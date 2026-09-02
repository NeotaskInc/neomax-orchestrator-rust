use chrono::{DateTime, Utc};

use crate::accounts::{AccountControlStore, AccountSnapshot, LiveWorkSource, QuotaSnapshotSource};
use crate::providers::{ProviderRegistry, runtime::ProviderRuntime};
use crate::{Result, WorkerScope};

pub struct AccountInventory<'a> {
    pub providers: &'a ProviderRegistry,
    pub quota: &'a dyn QuotaSnapshotSource,
    pub controls: &'a AccountControlStore,
    pub live_work: &'a dyn LiveWorkSource,
}

impl AccountInventory<'_> {
    pub fn from_runtime<'a>(
        runtime: &'a ProviderRuntime,
        quota: &'a dyn QuotaSnapshotSource,
        controls: &'a AccountControlStore,
        live_work: &'a dyn LiveWorkSource,
    ) -> AccountInventory<'a> {
        AccountInventory {
            providers: runtime.registry(),
            quota,
            controls,
            live_work,
        }
    }

    pub fn snapshots(
        &self,
        scope: &WorkerScope,
        now: DateTime<Utc>,
    ) -> Result<Vec<AccountSnapshot>> {
        let live = self.live_work.live_work()?;
        let mut snapshots = Vec::new();
        for engine in scope.engines() {
            if self.providers.get(engine).is_none() {
                continue;
            }
            for profile in self.providers.profiles_for(engine)? {
                let mut snapshot = AccountSnapshot {
                    engine,
                    account: profile.account.clone(),
                    profile: profile.path.clone(),
                    binary_available: self.providers.binary_available(engine),
                    authenticated: self.providers.managed_pool_eligible(&profile),
                    rotation_eligible: self.providers.rotation_eligible(&profile),
                    paused: self.controls.is_paused(&profile.path)?,
                    reserved: profile.reserved,
                    live_workers: live.count(engine, &profile.path),
                    five_hour_percent: None,
                    weekly_percent: None,
                    cooldown_until: self
                        .controls
                        .cooldown_until(&profile.path, now.timestamp_millis() as f64 / 1000.0)?
                        .and_then(epoch),
                    five_hour_reset_at: None,
                    weekly_reset_at: None,
                };
                let quota = self.quota.quota_snapshot(engine, &profile.path);
                snapshot.apply_quota(&quota, now);
                snapshots.push(snapshot);
            }
        }
        Ok(snapshots)
    }

    /// Builds the account view used by any new-work or failover selector.
    ///
    /// The full `snapshots` view remains available to status and portal
    /// consumers, including authenticated profiles whose executable is absent.
    /// Routing receives only profiles backed by a discovered executable.
    pub fn routing_snapshots(
        &self,
        scope: &WorkerScope,
        now: DateTime<Utc>,
    ) -> Result<Vec<AccountSnapshot>> {
        Ok(self
            .snapshots(scope, now)?
            .into_iter()
            .filter(|account| account.binary_available)
            .collect())
    }
}

fn epoch(value: f64) -> Option<DateTime<Utc>> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    DateTime::from_timestamp_millis((value * 1000.0) as i64)
}
