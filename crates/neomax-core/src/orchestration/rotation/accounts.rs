use crate::accounts::{AccountControlStore, AccountSnapshot};
use crate::orchestration::auth::{RotationEffects, RotationPaths, RotationService};
use crate::orchestration::handoff::{HandoffTargetRequest, TargetPolicy, select_target};
use crate::usage::{ProviderUsageCache, UsageCacheStore};
use crate::{Engine, Error, Result};
use chrono::{DateTime, Utc};
use std::path::Path;

pub fn choose_rotation_target(
    accounts: &[AccountSnapshot],
    source: &AccountSnapshot,
    selectors: &[String],
    now: DateTime<Utc>,
    policy: &TargetPolicy,
) -> Result<AccountSnapshot> {
    if !source.rotation_eligible {
        return Err(Error::Conflict(
            "this account does not support in-place OAuth rotation; use a session handoff".into(),
        ));
    }
    let candidates = accounts
        .iter()
        .filter(|account| account.rotation_eligible)
        .cloned()
        .collect::<Vec<_>>();
    Ok(select_target(&HandoffTargetRequest {
        accounts: &candidates,
        engine: source.engine,
        current_profile: &source.profile,
        selectors,
        now,
        policy,
    })?
    .account)
}

pub fn swap_account_auth(
    engine: Engine,
    first: &Path,
    second: &Path,
    paths: RotationPaths,
    controls: &AccountControlStore,
    now: i64,
) -> Result<RotationEffects> {
    swap_account_auth_with_reason(
        engine,
        first,
        second,
        paths,
        controls,
        now,
        Some("account-targeted CLI rotation".into()),
    )
}

fn swap_account_auth_with_reason(
    engine: Engine,
    first: &Path,
    second: &Path,
    paths: RotationPaths,
    controls: &AccountControlStore,
    now: i64,
    reason: Option<String>,
) -> Result<RotationEffects> {
    crate::orchestration::auth::copy_allowed(engine)?;
    crate::atomic::with_exclusive_lock(&paths.backup_dir.join("account-metadata.lock"), || {
        swap_account_auth_locked(engine, first, second, paths.clone(), controls, now, reason)
    })
}

fn swap_account_auth_locked(
    engine: Engine,
    first: &Path,
    second: &Path,
    paths: RotationPaths,
    controls: &AccountControlStore,
    now: i64,
    reason: Option<String>,
) -> Result<RotationEffects> {
    let usage = paths.usage_cache_dir.as_ref().map(UsageCacheStore::new);
    let first_cache = usage
        .as_ref()
        .and_then(|usage| usage.load_read_only(engine, first));
    let second_cache = usage
        .as_ref()
        .and_then(|usage| usage.load_read_only(engine, second));
    let service = RotationService::filesystem(paths);
    let effects = service.swap(engine, first, second, now, reason)?;
    let moved = (|| {
        if let Some(usage) = usage.as_ref() {
            save_quota(usage, engine, first, second_cache.as_ref())?;
            save_quota(usage, engine, second, first_cache.as_ref())?;
        }
        controls.swap_cooldowns(first, second)
    })();
    if let Err(error) = moved {
        let rollback = service.swap(
            engine,
            first,
            second,
            now,
            Some("rollback after account metadata failure".into()),
        );
        if let Some(usage) = usage.as_ref() {
            let _ = save_quota(usage, engine, first, first_cache.as_ref());
            let _ = save_quota(usage, engine, second, second_cache.as_ref());
        }
        return Err(Error::Message(format!(
            "account metadata update failed: {error}; credential rollback {}",
            if rollback.is_ok() {
                "completed"
            } else {
                "failed; use the private backups"
            }
        )));
    }
    Ok(effects)
}

pub struct AccountRotationService {
    pub paths: RotationPaths,
    pub controls: AccountControlStore,
}

impl crate::orchestration::continuation::CredentialRotationPort for AccountRotationService {
    fn supports(&self, engine: Engine) -> bool {
        crate::orchestration::auth::copy_allowed(engine).is_ok()
    }

    fn swap(
        &self,
        engine: Engine,
        destination: &Path,
        source: &Path,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects> {
        swap_account_auth_with_reason(
            engine,
            destination,
            source,
            self.paths.clone(),
            &self.controls,
            timestamp,
            reason,
        )
    }
}

fn save_quota(
    usage: &UsageCacheStore,
    engine: Engine,
    profile: &Path,
    cache: Option<&ProviderUsageCache>,
) -> Result<()> {
    usage.save(
        engine,
        profile,
        &cache.cloned().unwrap_or(ProviderUsageCache {
            stale: true,
            ..Default::default()
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_moves_quota_and_cooldowns_with_credentials_and_rolls_back_metadata_failure() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        for (profile, token) in [(&first, "fixture-first"), (&second, "fixture-second")] {
            std::fs::create_dir(profile).unwrap();
            std::fs::write(
                profile.join("auth.json"),
                serde_json::json!({"tokens":{"access_token":token,"refresh_token":token}})
                    .to_string(),
            )
            .unwrap();
        }
        let paths = RotationPaths::new(
            temp.path().join("backups"),
            temp.path().join("rotations.jsonl"),
        )
        .with_usage_cache_dir(temp.path().join("usage"));
        let usage = UsageCacheStore::new(temp.path().join("usage"));
        for (profile, percent) in [(&first, 95.0), (&second, 10.0)] {
            usage
                .save(
                    Engine::Codex,
                    profile,
                    &ProviderUsageCache {
                        five_hour: crate::usage::QuotaWindow {
                            used_percent: Some(percent),
                            resets_at: None,
                        },
                        observed_at: Some(1000.0),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let cooldowns = temp.path().join("cooldowns.json");
        let controls = AccountControlStore::new(&cooldowns, temp.path().join("paused.json"));
        controls
            .set_cooldown(&first, Some(1500.0), 1000.0, 300.0)
            .unwrap();
        swap_account_auth(
            Engine::Codex,
            &first,
            &second,
            paths.clone(),
            &controls,
            1000,
        )
        .unwrap();
        assert_eq!(
            usage
                .load_read_only(Engine::Codex, &first)
                .unwrap()
                .five_hour
                .used_percent,
            Some(10.0)
        );
        assert_eq!(
            usage
                .load_read_only(Engine::Codex, &second)
                .unwrap()
                .five_hour
                .used_percent,
            Some(95.0)
        );
        assert_eq!(controls.cooldown_until(&first, 1000.0).unwrap(), None);
        assert_eq!(
            controls.cooldown_until(&second, 1000.0).unwrap(),
            Some(1500.0)
        );
        let before_first = std::fs::read(first.join("auth.json")).unwrap();
        let before_second = std::fs::read(second.join("auth.json")).unwrap();
        let bad_controls = AccountControlStore::new(temp.path(), temp.path().join("paused.json"));
        assert!(
            swap_account_auth(
                Engine::Codex,
                &first,
                &second,
                paths.clone(),
                &bad_controls,
                1001
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(first.join("auth.json")).unwrap(),
            before_first
        );
        assert_eq!(
            std::fs::read(second.join("auth.json")).unwrap(),
            before_second
        );
        assert_eq!(
            usage
                .load_read_only(Engine::Codex, &first)
                .unwrap()
                .five_hour
                .used_percent,
            Some(10.0)
        );
        let invalid_log = RotationPaths::new(temp.path().join("backups"), temp.path());
        assert!(
            RotationService::filesystem(invalid_log)
                .swap(Engine::Codex, &first, &second, 1002, None)
                .is_err()
        );
        assert_eq!(
            std::fs::read(first.join("auth.json")).unwrap(),
            before_first
        );
        assert_eq!(
            std::fs::read(second.join("auth.json")).unwrap(),
            before_second
        );
    }
}
