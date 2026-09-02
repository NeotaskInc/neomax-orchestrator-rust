use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::atomic::{read_json_or_default, update_json_locked};
use crate::Result;

use super::selection::{compare_account_rank, rank_account, AccountRankingPolicy};
use super::snapshot::AccountSnapshot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationRank {
    pub over_five_hour_ceiling: bool,
    pub spread_load: f64,
    pub weekly_percent: f64,
}

pub struct RotationClaimStore {
    claims: PathBuf,
    lock: PathBuf,
    claim_ttl_seconds: f64,
    ranking: AccountRankingPolicy,
}

impl RotationClaimStore {
    pub fn new(claims: impl Into<PathBuf>, lock: impl Into<PathBuf>) -> Self {
        Self {
            claims: claims.into(),
            lock: lock.into(),
            claim_ttl_seconds: 120.0,
            ranking: AccountRankingPolicy::default(),
        }
    }

    pub fn claims(&self) -> BTreeMap<String, f64> {
        read_json_or_default(&self.claims)
    }

    pub fn claim_count(&self, profile: &Path, now: f64) -> u32 {
        let Some(key) = profile_key(profile) else {
            return 0;
        };
        self.claims()
            .get(&key)
            .is_some_and(|timestamp| now - timestamp < self.claim_ttl_seconds)
            .into()
    }

    pub fn try_claim(&self, profile: &Path, now: DateTime<Utc>) -> Result<bool> {
        let key = profile_key(profile).ok_or_else(|| {
            crate::Error::InvalidArgument(format!(
                "profile path must not be rooted without an absolute prefix: {}",
                profile.display()
            ))
        })?;
        let now_epoch = now.timestamp_millis() as f64 / 1000.0;
        let mut claimed = false;
        update_json_locked::<BTreeMap<String, f64>, _>(&self.claims, &self.lock, |claims| {
            claims.retain(|_, timestamp| now_epoch - *timestamp < self.claim_ttl_seconds);
            if claims
                .get(&key)
                .is_some_and(|timestamp| now_epoch - timestamp < self.claim_ttl_seconds)
            {
                return Ok(());
            }
            claims.insert(key.clone(), now_epoch);
            claimed = true;
            Ok(())
        })?;
        Ok(claimed)
    }

    pub fn release(&self, profile: &Path) -> Result<bool> {
        let Some(key) = profile_key(profile) else {
            return Ok(false);
        };
        let mut released = false;
        update_json_locked::<BTreeMap<String, f64>, _>(&self.claims, &self.lock, |claims| {
            released = claims.remove(&key).is_some();
            Ok(())
        })?;
        Ok(released)
    }

    pub fn rank(
        &self,
        account: &AccountSnapshot,
        claims: &BTreeMap<String, f64>,
        now: DateTime<Utc>,
        include_claims: bool,
    ) -> RotationRank {
        let claimed = include_claims
            && profile_key(&account.profile).is_some_and(|key| {
                claims.get(&key).is_some_and(|timestamp| {
                    now.timestamp_millis() as f64 / 1000.0 - timestamp < self.claim_ttl_seconds
                })
            });
        let contention = account.live_workers + u32::from(claimed);
        let rank = rank_account(account, now, contention, &self.ranking);
        RotationRank {
            over_five_hour_ceiling: rank.at_five_hour_hard_wall,
            spread_load: rank.score,
            weekly_percent: rank.weekly_percent,
        }
    }

    pub fn pick_and_claim<'a>(
        &self,
        accounts: &'a [AccountSnapshot],
        now: DateTime<Utc>,
    ) -> Result<Option<&'a AccountSnapshot>> {
        if accounts.is_empty() {
            return Ok(None);
        }
        let mut selected_index = None;
        let now_epoch = now.timestamp_millis() as f64 / 1000.0;
        update_json_locked::<BTreeMap<String, f64>, _>(&self.claims, &self.lock, |claims| {
            claims.retain(|_, timestamp| now_epoch - *timestamp < self.claim_ttl_seconds);
            selected_index = accounts
                .iter()
                .enumerate()
                .filter(|(_, account)| {
                    account.binary_available && profile_key(&account.profile).is_some()
                })
                .min_by(|(_, left), (_, right)| {
                    compare_rank(
                        self.rank(left, claims, now, true),
                        self.rank(right, claims, now, true),
                    )
                    .then_with(|| left.account.cmp(&right.account))
                })
                .map(|(index, _)| index);
            if let Some(index) = selected_index {
                let key = profile_key(&accounts[index].profile)
                    .expect("selected profiles have a valid absolute or relative path");
                claims.insert(key, now_epoch);
            }
            Ok(())
        })?;
        Ok(selected_index.map(|index| &accounts[index]))
    }
}

fn profile_key(profile: &Path) -> Option<String> {
    if crate::io::is_rooted_but_not_absolute(profile) {
        return None;
    }
    if profile.is_absolute() {
        Some(profile.to_string_lossy().into_owned())
    } else {
        Some(
            std::env::current_dir()
                .map(|cwd| cwd.join(profile))
                .unwrap_or_else(|_| profile.to_path_buf())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn compare_rank(left: RotationRank, right: RotationRank) -> std::cmp::Ordering {
    compare_account_rank(
        super::selection::AccountRank {
            at_five_hour_hard_wall: left.over_five_hour_ceiling,
            score: left.spread_load,
            weekly_percent: left.weekly_percent,
        },
        super::selection::AccountRank {
            at_five_hour_hard_wall: right.over_five_hour_ceiling,
            score: right.spread_load,
            weekly_percent: right.weekly_percent,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    fn account(name: &str) -> AccountSnapshot {
        AccountSnapshot {
            engine: Engine::Claude,
            account: name.into(),
            profile: PathBuf::from(name),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: Some(10.0),
            weekly_percent: Some(10.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        }
    }

    #[test]
    fn serialized_claims_spread_concurrent_rotation_targets() {
        let temp = tempfile::tempdir().unwrap();
        let store = RotationClaimStore::new(
            temp.path().join("rotation-claims.json"),
            temp.path().join("rotation.lock"),
        );
        let accounts = [account("1"), account("2")];
        let now = Utc::now();
        assert_eq!(
            store
                .pick_and_claim(&accounts, now)
                .unwrap()
                .unwrap()
                .account,
            "1"
        );
        assert_eq!(
            store
                .pick_and_claim(&accounts, now)
                .unwrap()
                .unwrap()
                .account,
            "2"
        );
    }

    #[test]
    fn try_claim_is_single_owner_and_release_is_atomic() {
        let temp = tempfile::tempdir().unwrap();
        let store = RotationClaimStore::new(
            temp.path().join("rotation-claims.json"),
            temp.path().join("rotation.lock"),
        );
        let profile = temp.path().join("profile");
        let now = Utc::now();
        assert!(store.try_claim(&profile, now).unwrap());
        assert!(!store.try_claim(&profile, now).unwrap());
        assert_eq!(store.claim_count(&profile, now.timestamp() as f64), 1);
        assert!(store.release(&profile).unwrap());
        assert!(!store.release(&profile).unwrap());
        assert_eq!(store.claim_count(&profile, now.timestamp() as f64), 0);
    }

    #[test]
    fn concurrent_try_claims_have_one_owner() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(RotationClaimStore::new(
            temp.path().join("rotation-claims.json"),
            temp.path().join("rotation.lock"),
        ));
        let profile = Arc::new(temp.path().join("profile"));
        let now = Utc::now();
        let barrier = Arc::new(Barrier::new(8));
        let owners = thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| {
                    let store = Arc::clone(&store);
                    let profile = Arc::clone(&profile);
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        store.try_claim(&profile, now).unwrap()
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|claimed| *claimed)
                .count()
        });
        assert_eq!(owners, 1);
    }

    #[cfg(windows)]
    #[test]
    fn partial_root_profiles_cannot_claim_or_compete_for_rotation() {
        let temp = tempfile::tempdir().unwrap();
        let store = RotationClaimStore::new(
            temp.path().join("rotation-claims.json"),
            temp.path().join("rotation.lock"),
        );
        let now = Utc::now();

        for raw in [r"\rooted", r"C:drive-relative"] {
            let profile = Path::new(raw);
            assert!(store.try_claim(profile, now).is_err());
            assert_eq!(store.claim_count(profile, now.timestamp() as f64), 0);
            assert!(!store.release(profile).unwrap());
        }

        let mut invalid = account("invalid");
        invalid.profile = PathBuf::from(r"\rooted");
        assert!(store.pick_and_claim(&[invalid], now).unwrap().is_none());
    }
}
