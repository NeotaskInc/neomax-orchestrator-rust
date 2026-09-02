use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::io::is_rooted_but_not_absolute;

pub(super) fn source_account(
    engine: Engine,
    current_profile: &Path,
    requested: Option<&str>,
    accounts: &[AccountSnapshot],
    environment: &BTreeMap<String, String>,
    home: &Path,
) -> Result<AccountSnapshot> {
    if is_rooted_but_not_absolute(current_profile) {
        bail!(
            "handoff source profile must not be rooted without an absolute prefix: {}",
            current_profile.display()
        );
    }
    if is_rooted_but_not_absolute(home) {
        bail!(
            "handoff profile home must not be rooted without an absolute prefix: {}",
            home.display()
        );
    }
    if let Some(account) = accounts
        .iter()
        .filter(|account| !is_rooted_but_not_absolute(&account.profile))
        .find(|account| account.engine == engine && same_path(&account.profile, current_profile))
    {
        let mut source = account.clone();
        if let Some(requested) = requested.filter(|value| !value.trim().is_empty()) {
            source.account = requested.to_owned();
        }
        return Ok(source);
    }
    let account = requested
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            (environment.get("NEOMAX_ORCH_RESERVED").map(String::as_str) == Some("1"))
                .then_some("orch".into())
        })
        .or_else(|| {
            neomax_core::providers::catalog::profile_account_number(engine, current_profile)
                .map(|number| number.to_string())
        })
        .unwrap_or_else(|| {
            current_profile
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(match engine {
                    Engine::Claude => ".claude",
                    Engine::Codex => ".codex",
                    Engine::Opencode => ".opencode",
                    Engine::Kimi => ".kimi-code",
                    Engine::Grok => ".grok",
                })
                .to_owned()
        });
    let reserved = account.eq_ignore_ascii_case("orch")
        || environment.get("NEOMAX_ORCH_RESERVED").map(String::as_str) == Some("1");
    let profile = if current_profile.is_absolute() {
        current_profile.to_path_buf()
    } else {
        home.join(current_profile)
    };
    if is_rooted_but_not_absolute(&profile) {
        bail!(
            "handoff source profile must not be rooted without an absolute prefix: {}",
            profile.display()
        );
    }
    Ok(AccountSnapshot {
        engine,
        account,
        profile,
        binary_available: false,
        authenticated: true,
        rotation_eligible: false,
        paused: false,
        reserved,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    })
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    !is_rooted_but_not_absolute(left)
        && !is_rooted_but_not_absolute(right)
        && normalize(left) == normalize(right)
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::RootDir
            | std::path::Component::Prefix(_)
            | std::path::Component::Normal(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(super) fn reset_label(reset: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<String> {
    let reset = reset?;
    let seconds = reset.signed_duration_since(now).num_seconds();
    if seconds <= 0 {
        return None;
    }
    let hours = (seconds + 3_599) / 3_600;
    if hours >= 48 {
        Some(format!("~{}d", (hours + 23) / 24))
    } else {
        Some(format!("~{}h", hours))
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn rejects_windows_partial_root_source_profiles_before_home_joining() {
        let home = Path::new(r"C:\Users\fixture");
        let environment = BTreeMap::new();
        for raw in [r"\rooted", r"C:drive-relative"] {
            let error = source_account(
                Engine::Claude,
                Path::new(raw),
                None,
                &[],
                &environment,
                home,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
    }
}
