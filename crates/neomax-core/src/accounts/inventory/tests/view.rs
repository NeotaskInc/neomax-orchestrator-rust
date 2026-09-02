use std::collections::BTreeMap;

use chrono::{Duration, Utc};

use crate::accounts::{AccountControlStore, LiveWorkSnapshot, QuotaSnapshot};
use crate::{Engine, WorkerScope};

use super::super::AccountInventory;
use super::support::{LiveWorkFixture, ProviderFixture, QuotaFixture};
use crate::providers::{Provider, ProviderProfile, ProviderRegistry};

#[test]
fn builds_one_shared_view_of_auth_quota_controls_and_live_work() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("profiles/codex1");
    let second = temp.path().join("profiles/codex2");
    let providers = ProviderRegistry::new([Box::new(ProviderFixture {
        profiles: vec![
            ProviderProfile {
                engine: Engine::Codex,
                account: "1".into(),
                path: first.clone(),
                reserved: false,
            },
            ProviderProfile {
                engine: Engine::Codex,
                account: "2".into(),
                path: second.clone(),
                reserved: false,
            },
        ],
    }) as Box<dyn Provider>]);
    let now = Utc::now();
    let quota = QuotaFixture {
        snapshots: BTreeMap::from([(
            (Engine::Codex, first.clone()),
            QuotaSnapshot {
                available: true,
                weekly_percent: Some(74.0),
                weekly_reset_at: Some(now + Duration::days(2)),
                ..QuotaSnapshot::default()
            },
        )]),
    };
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    controls.set_paused(&second, true).unwrap();
    controls
        .set_cooldown(
            &first,
            Some((now + Duration::hours(1)).timestamp() as f64),
            now.timestamp() as f64,
            1_800.0,
        )
        .unwrap();
    let live = LiveWorkSnapshot {
        counts: BTreeMap::from([((Engine::Codex, first.clone()), 1)]),
    };
    let live_work = LiveWorkFixture { snapshot: live };
    let inventory = AccountInventory {
        providers: &providers,
        quota: &quota,
        controls: &controls,
        live_work: &live_work,
    };
    let snapshots = inventory
        .snapshots(&WorkerScope::only(Engine::Codex), now)
        .unwrap();
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots[0].authenticated);
    assert_eq!(snapshots[0].weekly_percent, Some(74.0));
    assert_eq!(snapshots[0].live_workers, 1);
    assert!(snapshots[0].cooldown_until.is_some());
    assert!(!snapshots[1].authenticated);
    assert!(snapshots[1].paused);
}
