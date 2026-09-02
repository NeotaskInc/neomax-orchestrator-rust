use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(not(target_os = "macos"))]
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::DateTime;
use neomax_core::atomic::write_json_atomic;
use neomax_core::config::Engine;
#[cfg(target_os = "macos")]
use neomax_core::providers::auth::checked_keychain_service;
#[cfg(target_os = "macos")]
use neomax_core::providers::scrub_provider_environment;
use neomax_core::usage::{ProviderUsageCache, QuotaWindow};
use serde_json::Value;

use super::cache::load;
use super::cache::{DEFAULT_FRESH_SECS, DEFAULT_STALE_SECS, fresh, now_epoch, stale_ok};
use super::http::JsonHttp;
use crate::config::AgentPaths;
use crate::io::{MAX_CREDENTIAL_BYTES, read_bounded};

pub(crate) const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub(crate) const CLAUDE_OAUTH_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_OAUTH_USER_AGENT: &str = "claude-cli/2.1.56 (external, cli)";
pub(crate) const CLAUDE_TIMEOUT: Duration = Duration::from_secs(8);

pub(crate) fn refresh(
    paths: &AgentPaths,
    profile: &Path,
    http: &dyn JsonHttp,
    force: bool,
    use_keychain: bool,
) -> Result<Option<ProviderUsageCache>> {
    let store = neomax_core::usage::UsageCacheStore::new(&paths.state.usage);
    let now = now_epoch();
    let account_uuid = local_identity_uuid(profile, &paths.home);
    let cached = load(&store, Engine::Claude, profile)
        .filter(|cache| cache_matches_account(cache, account_uuid.as_deref()));
    if !force
        && cached.as_ref().is_some_and(|cache| {
            fresh(
                cache,
                now,
                "claude-api",
                Duration::from_secs(DEFAULT_FRESH_SECS as u64),
            )
        })
    {
        return Ok(cached);
    }
    let credentials = ClaudeCredentials::read(profile, &paths.home, use_keychain);
    let token = credentials
        .access_token(now)
        .map(str::to_owned)
        .or_else(|| {
            credentials
                .refresh_token()
                .and_then(|refresh| refresh_access_token(profile, refresh, http, now))
        });
    let Some(token) = token else {
        if let Some(mut cache) = cached {
            if stale_ok(&cache, now, Duration::from_secs(DEFAULT_STALE_SECS as u64)) {
                cache.stale = true;
                return Ok(Some(cache));
            }
            cache.expired = true;
            cache.recoverable = credentials.has_refresh_token;
            return Ok(Some(cache));
        }
        if credentials.has_refresh_token {
            return Ok(Some(ProviderUsageCache {
                source: Some("claude-api".into()),
                observed_at: Some(now as f64),
                expired: true,
                recoverable: true,
                ..ProviderUsageCache::default()
            }));
        }
        return Ok(None);
    };
    let authorization = format!("Bearer {token}");
    let response = match http.get_json(
        CLAUDE_USAGE_URL,
        &[
            ("Authorization", authorization.as_str()),
            ("anthropic-beta", "oauth-2025-04-20"),
            ("anthropic-version", "2023-06-01"),
        ],
        CLAUDE_TIMEOUT,
    ) {
        Ok(response) => response,
        Err(_) => {
            return Ok(cached.and_then(|mut cache| {
                stale_ok(&cache, now, Duration::from_secs(DEFAULT_STALE_SECS as u64)).then(|| {
                    cache.stale = true;
                    cache
                })
            }));
        }
    };
    let mut output = ProviderUsageCache {
        five_hour: window(response.get("five_hour")),
        seven_day: window(response.get("seven_day")),
        source: Some("claude-api".into()),
        observed_at: Some(now as f64),
        ..ProviderUsageCache::default()
    };
    if let Some(account_uuid) = account_uuid {
        output
            .extra
            .insert("acct_uuid".into(), Value::String(account_uuid));
    }
    if output.five_hour.used_percent.is_none() && output.seven_day.used_percent.is_none() {
        return Ok(cached.and_then(|mut cache| {
            stale_ok(&cache, now, Duration::from_secs(DEFAULT_STALE_SECS as u64)).then(|| {
                cache.stale = true;
                cache
            })
        }));
    }
    Ok(Some(output))
}

fn refresh_access_token(
    profile: &Path,
    refresh_token: &str,
    http: &dyn JsonHttp,
    now: i64,
) -> Option<String> {
    let response = http
        .post_json(
            CLAUDE_OAUTH_TOKEN_URL,
            &[
                ("Content-Type", "application/json"),
                ("Accept", "application/json"),
                ("User-Agent", CLAUDE_OAUTH_USER_AGENT),
            ],
            &serde_json::json!({
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
                "client_id": CLAUDE_OAUTH_CLIENT_ID,
            }),
            CLAUDE_TIMEOUT,
        )
        .ok()?;
    let access_token = response.get("access_token")?.as_str()?.to_owned();
    if access_token.is_empty() {
        return None;
    }
    let expires_at = response
        .get("expires_in")
        .and_then(number)
        .and_then(|seconds| {
            let millis = (now as f64 + seconds) * 1000.0;
            (millis.is_finite() && millis > 0.0 && millis <= i64::MAX as f64)
                .then_some(millis as i64)
        });
    let replacement_refresh = response
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());
    persist_credentials(profile, &access_token, replacement_refresh, expires_at);
    Some(access_token)
}

fn persist_credentials(
    profile: &Path,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<i64>,
) {
    let path = profile.join(".credentials.json");
    let Ok(bytes) = read_bounded(&path, MAX_CREDENTIAL_BYTES) else {
        return;
    };
    let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    let Some(oauth) = value
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    oauth.insert("accessToken".into(), Value::String(access_token.into()));
    if let Some(refresh_token) = refresh_token {
        oauth.insert("refreshToken".into(), Value::String(refresh_token.into()));
    }
    if let Some(expires_at) = expires_at {
        oauth.insert("expiresAt".into(), Value::Number(expires_at.into()));
    }
    let _ = write_json_atomic(&path, &value);
}

fn local_identity_uuid(profile: &Path, home: &Path) -> Option<String> {
    let path = neomax_core::orchestration::auth::claude::identity_path_for_home(profile, home);
    let bytes = read_bounded(&path, MAX_CREDENTIAL_BYTES).ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    value
        .pointer("/oauthAccount/accountUuid")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn cache_matches_account(cache: &ProviderUsageCache, account_uuid: Option<&str>) -> bool {
    let Some(account_uuid) = account_uuid else {
        return true;
    };
    cache
        .extra
        .get("acct_uuid")
        .and_then(Value::as_str)
        .is_some_and(|cached_uuid| cached_uuid == account_uuid)
}

#[derive(Debug, Default)]
struct ClaudeCredentials {
    blobs: Vec<Value>,
    has_refresh_token: bool,
}

impl ClaudeCredentials {
    fn read(profile: &Path, home: &Path, use_keychain: bool) -> Self {
        let mut blobs = Vec::new();
        if let Ok(bytes) = read_bounded(&profile.join(".credentials.json"), MAX_CREDENTIAL_BYTES) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                blobs.push(value);
            }
        }
        if use_keychain {
            blobs.extend(keychain_blobs(profile, home));
        }
        let has_refresh_token = blobs.iter().any(|value| {
            value
                .get("claudeAiOauth")
                .and_then(Value::as_object)
                .and_then(|oauth| oauth.get("refreshToken"))
                .is_some_and(json_truthy)
        });
        Self {
            blobs,
            has_refresh_token,
        }
    }

    fn access_token(&self, now: i64) -> Option<&str> {
        self.blobs.iter().find_map(|value| {
            let oauth = value.get("claudeAiOauth")?.as_object()?;
            let token = oauth.get("accessToken")?.as_str()?;
            let expires = oauth.get("expiresAt").and_then(number);
            if expires.is_some_and(|value| normalize_epoch(value) <= now as f64) {
                return None;
            }
            Some(token)
        })
    }

    fn refresh_token(&self) -> Option<&str> {
        self.blobs.iter().find_map(|value| {
            value
                .get("claudeAiOauth")?
                .get("refreshToken")?
                .as_str()
                .filter(|token| !token.is_empty())
        })
    }
}

#[cfg(target_os = "macos")]
fn keychain_blobs(profile: &Path, home: &Path) -> Vec<Value> {
    let Ok(service) = checked_keychain_service(profile, home) else {
        return Vec::new();
    };
    let mut commands = vec![vec!["-s".to_owned(), service.clone(), "-w".to_owned()]];
    if let Some(user) = std::env::var_os("USER").and_then(|value| value.into_string().ok()) {
        commands.insert(
            0,
            vec![
                "-a".to_owned(),
                user,
                "-s".to_owned(),
                service,
                "-w".to_owned(),
            ],
        );
    }
    commands
        .into_iter()
        .filter_map(|args| {
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            bounded_security(&args)
        })
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn keychain_blobs(_profile: &Path, _home: &Path) -> Vec<Value> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn bounded_security(args: &[&str]) -> Option<String> {
    let mut command = Command::new("security");
    scrub_provider_environment(&mut command);
    let mut child = command
        .args(["find-generic-password"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader =
        thread::spawn(move || crate::io::read_capped(&mut stdout, MAX_CREDENTIAL_BYTES as usize));
    let start = Instant::now();
    loop {
        if child.try_wait().ok()?.is_some() {
            let status = child.wait().ok()?;
            let (bytes, exceeded) = reader.join().ok()?.ok()?;
            if status.success() && !exceeded {
                return String::from_utf8(bytes).ok();
            }
            return None;
        }
        if start.elapsed() >= CLAUDE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn window(value: Option<&Value>) -> QuotaWindow {
    let Some(object) = value.and_then(Value::as_object) else {
        return QuotaWindow::default();
    };
    QuotaWindow {
        used_percent: object.get("utilization").and_then(number),
        resets_at: object.get("resets_at").and_then(reset_epoch),
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn reset_epoch(value: &Value) -> Option<f64> {
    number(value).map(normalize_epoch).or_else(|| {
        value
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp_millis() as f64 / 1000.0)
    })
}

fn normalize_epoch(mut value: f64) -> f64 {
    while value > 100_000_000_000.0 {
        value /= 1000.0;
    }
    value
}

fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
