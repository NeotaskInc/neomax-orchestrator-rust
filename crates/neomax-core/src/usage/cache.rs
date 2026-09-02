use std::collections::BTreeMap;
use std::env;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::accounts::{
    quota_support, AccountSnapshot, QuotaSnapshot, QuotaSnapshotSource, QuotaSupport,
};
use crate::atomic::write_json_atomic;
use crate::io::{read_file, LocalFileSource, ReadLimits};
use crate::runtime::RuntimeEnvironment;
use crate::{Engine, Result};

const MAX_CACHE_BYTES: usize = 512 * 1024;
const CACHE_READ_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_IDENTITY_DOMAIN: &[u8] = b"neomax-usage-profile-v2\0";
const INVALID_CACHE_IDENTITY_DOMAIN: &[u8] = b"neomax-invalid-usage-profile-v1\0";
const CACHE_PROFILE_IDENTITY_KEY: &str = "neomax_profile_identity";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuotaWindow {
    #[serde(default, deserialize_with = "optional_number")]
    pub used_percent: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub resets_at: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageCache {
    #[serde(default)]
    pub five_hour: QuotaWindow,
    #[serde(default)]
    pub seven_day: QuotaWindow,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, deserialize_with = "optional_number")]
    pub observed_at: Option<f64>,
    #[serde(default)]
    pub expired: bool,
    #[serde(default)]
    pub recoverable: bool,
    #[serde(default)]
    pub stale: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl ProviderUsageCache {
    /// A reactive provider may not expose a native quota endpoint, but an
    /// event or provider-specific collector can still persist a weekly
    /// percentage. Only use that signal when its provenance and timestamp
    /// are present and the cache has not been marked unusable.
    pub fn has_trustworthy_weekly(&self) -> bool {
        self.seven_day
            .used_percent
            .is_some_and(|value| value.is_finite() && (0.0..=100.0).contains(&value))
            && self
                .source
                .as_deref()
                .is_some_and(|source| !source.trim().is_empty())
            && self
                .observed_at
                .is_some_and(|value| value.is_finite() && value >= 0.0)
            && !self.expired
            && !self.stale
    }
}

pub struct UsageCacheStore {
    directory: PathBuf,
}

impl UsageCacheStore {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    pub fn path(&self, engine: Engine, profile: &Path) -> PathBuf {
        self.directory.join(format!(
            "{}-{}.json",
            engine.as_str(),
            profile_identity(profile).unwrap_or_else(|| invalid_profile_identity(profile))
        ))
    }

    pub fn cache_paths(&self, engine: Engine, profile: &Path) -> [PathBuf; 2] {
        [
            self.path(engine, profile),
            legacy_path(&self.directory, engine, profile),
        ]
    }

    pub fn load(&self, engine: Engine, profile: &Path) -> Option<ProviderUsageCache> {
        let identity = profile_identity(profile)?;
        let [path, legacy] = self.cache_paths(engine, profile);
        let limits = ReadLimits::new(MAX_CACHE_BYTES, CACHE_READ_TIMEOUT).ok()?;
        let native = read_cache_for_identity(&path, limits, &identity);
        let legacy_cache = read_cache_for_identity(&legacy, limits, &identity);

        if let Some(native) = native {
            if let Some(legacy_cache) = legacy_cache.filter(|cache| {
                legacy_is_newer(&path, &legacy) && caches_are_compatible(&native, cache)
            }) {
                let migrated = with_identity(&legacy_cache, &identity);
                if write_json_atomic(&path, &migrated).is_ok() {
                    return Some(strip_identity(migrated));
                }
            }
            return Some(strip_identity(native));
        }

        let cache = legacy_cache?;
        let migrated = with_identity(&cache, &identity);
        let _ = write_json_atomic(&legacy, &migrated);
        if write_json_atomic(&path, &migrated).is_ok() {
            return Some(strip_identity(migrated));
        }
        Some(strip_identity(cache))
    }

    pub fn save(&self, engine: Engine, profile: &Path, cache: &ProviderUsageCache) -> Result<()> {
        let identity = profile_identity(profile).ok_or_else(|| {
            crate::Error::InvalidArgument(format!(
                "profile path must not be rooted without an absolute prefix: {}",
                profile.display()
            ))
        })?;
        let cache = with_identity(cache, &identity);

        // Keep the basename file current for the Python implementation before
        // replacing the collision-safe native file.
        write_json_atomic(&legacy_path(&self.directory, engine, profile), &cache)?;
        write_json_atomic(&self.path(engine, profile), &cache)
    }

    pub fn hydrate(&self, account: &mut AccountSnapshot, now: DateTime<Utc>) {
        account.apply_quota(&self.quota_snapshot(account.engine, &account.profile), now);
    }
}

fn read_cache(path: &Path, limits: ReadLimits) -> Option<ProviderUsageCache> {
    let bytes = read_file(&LocalFileSource, path, limits).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn read_cache_for_identity(
    path: &Path,
    limits: ReadLimits,
    identity: &str,
) -> Option<ProviderUsageCache> {
    let cache = read_cache(path, limits)?;
    let stored_identity = cache
        .extra
        .get(CACHE_PROFILE_IDENTITY_KEY)
        .and_then(serde_json::Value::as_str);
    if stored_identity.is_some_and(|stored| stored != identity) {
        return None;
    }
    Some(cache)
}

fn with_identity(cache: &ProviderUsageCache, identity: &str) -> ProviderUsageCache {
    let mut tagged = cache.clone();
    tagged.extra.insert(
        CACHE_PROFILE_IDENTITY_KEY.into(),
        serde_json::Value::String(identity.into()),
    );
    tagged
}

fn strip_identity(mut cache: ProviderUsageCache) -> ProviderUsageCache {
    cache.extra.remove(CACHE_PROFILE_IDENTITY_KEY);
    cache
}

fn legacy_is_newer(native: &Path, legacy: &Path) -> bool {
    let Ok(native_modified) = std::fs::metadata(native).and_then(|metadata| metadata.modified())
    else {
        return false;
    };
    let Ok(legacy_modified) = std::fs::metadata(legacy).and_then(|metadata| metadata.modified())
    else {
        return false;
    };
    legacy_modified > native_modified
}

fn caches_are_compatible(native: &ProviderUsageCache, legacy: &ProviderUsageCache) -> bool {
    let native_account = native
        .extra
        .get("acct_uuid")
        .and_then(serde_json::Value::as_str);
    let legacy_account = legacy
        .extra
        .get("acct_uuid")
        .and_then(serde_json::Value::as_str);
    native_account.is_none() || legacy_account.is_none() || native_account == legacy_account
}

fn legacy_path(directory: &Path, engine: Engine, profile: &Path) -> PathBuf {
    let account = profile
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default");
    directory.join(format!("{}-{account}.json", engine.as_str()))
}

fn profile_identity(profile: &Path) -> Option<String> {
    let canonical = canonical_profile_path(profile)?;
    let mut digest = Sha256::new();
    digest.update(CACHE_IDENTITY_DOMAIN);
    digest.update(canonical.to_string_lossy().as_bytes());
    Some(format!("{:x}", digest.finalize()))
}

fn invalid_profile_identity(profile: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(INVALID_CACHE_IDENTITY_DOMAIN);
    digest.update(profile.as_os_str().to_string_lossy().as_bytes());
    format!("{:x}", digest.finalize())
}

fn canonical_profile_path(profile: &Path) -> Option<PathBuf> {
    let expanded = expand_home(profile);
    if crate::io::is_rooted_but_not_absolute(&expanded) {
        return None;
    }
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from(std::path::MAIN_SEPARATOR.to_string()))
            .join(expanded)
    };
    let absolute = lexical_normalize(&absolute);
    if let Ok(canonical) = absolute.canonicalize() {
        return Some(canonical);
    }

    let mut existing = absolute.clone();
    let mut suffix = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        suffix.push(name.to_os_string());
        if !existing.pop() {
            break;
        }
    }
    let mut resolved = existing.canonicalize().unwrap_or(existing);
    for name in suffix.iter().rev() {
        resolved.push(name);
    }
    Some(resolved)
}

fn expand_home(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if (text == "~" || text.starts_with("~/") || text.starts_with("~\\"))
        && RuntimeEnvironment::process().home_dir().is_some()
    {
        return RuntimeEnvironment::process().resolve_path(&text);
    }
    path.to_path_buf()
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                output.pop();
            }
            Component::Normal(value) => output.push(value),
        }
    }
    if output.as_os_str().is_empty() {
        PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
    } else {
        output
    }
}

impl QuotaSnapshotSource for UsageCacheStore {
    fn quota_snapshot(&self, engine: Engine, profile: &Path) -> QuotaSnapshot {
        let Some(cache) = self.load(engine, profile) else {
            return QuotaSnapshot::default();
        };
        let numeric = matches!(quota_support(engine), QuotaSupport::Numeric);
        let reactive_weekly = matches!(quota_support(engine), QuotaSupport::Reactive)
            && cache.has_trustworthy_weekly();
        if !numeric && !reactive_weekly {
            return QuotaSnapshot::default();
        }
        QuotaSnapshot {
            available: true,
            five_hour_percent: numeric.then_some(cache.five_hour.used_percent).flatten(),
            weekly_percent: cache.seven_day.used_percent,
            five_hour_reset_at: numeric
                .then_some(cache.five_hour.resets_at.and_then(epoch_datetime))
                .flatten(),
            weekly_reset_at: cache.seven_day.resets_at.and_then(epoch_datetime),
            expired: cache.expired,
        }
    }
}

fn optional_number<'de, D>(deserializer: D) -> std::result::Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::Number(value) => value.as_f64(),
        serde_json::Value::String(value) => value.parse().ok(),
        _ => None,
    }))
}

fn epoch_datetime(mut value: f64) -> Option<DateTime<Utc>> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    while value > 100_000_000_000.0 {
        value /= 1000.0;
    }
    DateTime::from_timestamp_millis((value * 1000.0) as i64)
}

#[cfg(test)]
#[path = "cache_tests/mod.rs"]
mod tests;
