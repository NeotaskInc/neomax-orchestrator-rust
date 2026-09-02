use std::collections::BTreeMap;

use chrono::Utc;

use crate::accounts::{AccountControlStore, LiveWorkSnapshot};
use crate::providers::catalog::{
    spec, AuthMethod, AuthStatus, BinaryStatus, CatalogSnapshot, ProfileEligibility,
    ProfileSnapshot, ProviderSnapshot,
};
use crate::providers::runtime::ProviderRuntime;
use crate::{Engine, WorkerScope};

use super::super::AccountInventory;
use super::support::{routing_runtime, LiveWorkFixture, QuotaFixture};

#[test]
fn includes_api_key_profiles_in_the_pool_for_every_provider() {
    let temp = tempfile::tempdir().unwrap();
    let providers = Engine::ALL
        .into_iter()
        .map(|engine| {
            let profile = temp.path().join(format!("{engine}-api-key"));
            (
                engine,
                ProviderSnapshot {
                    spec: spec(engine),
                    binary: BinaryStatus {
                        program: format!("{engine}-fixture"),
                        available: true,
                        version: Some("fixture".into()),
                    },
                    profiles: vec![ProfileSnapshot {
                        engine,
                        account: "api-key".into(),
                        path: profile,
                        reserved: false,
                        auth: AuthStatus::Authenticated {
                            methods: vec![AuthMethod::ApiKey],
                        },
                        eligibility: ProfileEligibility {
                            credential_present: true,
                            authenticated: true,
                            worker_eligible: true,
                            orchestrator_eligible: true,
                            rotation_eligible: false,
                            managed_pool_eligible: true,
                        },
                    }],
                    models: vec![spec(engine).default_model],
                },
            )
        })
        .collect();
    let runtime = ProviderRuntime::from_catalog(CatalogSnapshot { providers });
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let quota = QuotaFixture {
        snapshots: BTreeMap::new(),
    };
    let live_work = LiveWorkFixture {
        snapshot: LiveWorkSnapshot::default(),
    };
    let snapshots = AccountInventory::from_runtime(&runtime, &quota, &controls, &live_work)
        .snapshots(&WorkerScope::all(), Utc::now())
        .unwrap();

    assert_eq!(snapshots.len(), Engine::ALL.len());
    assert!(snapshots.iter().all(|snapshot| snapshot.authenticated));
    assert!(snapshots.iter().all(|snapshot| !snapshot.rotation_eligible));
    assert!(Engine::ALL.iter().all(|engine| {
        runtime
            .registry()
            .profiles_for(*engine)
            .unwrap()
            .iter()
            .all(|profile| runtime.registry().managed_pool_eligible(profile))
    }));
    assert!(Engine::ALL.iter().all(|engine| {
        runtime
            .registry()
            .profiles_for(*engine)
            .unwrap()
            .iter()
            .all(|profile| !runtime.registry().rotation_eligible(profile))
    }));
}

#[test]
fn routing_keeps_missing_binary_profiles_visible_but_never_returns_them() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = routing_runtime(temp.path(), false, true);
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let quota = QuotaFixture {
        snapshots: BTreeMap::new(),
    };
    let live_work = LiveWorkFixture {
        snapshot: LiveWorkSnapshot::default(),
    };
    let inventory = AccountInventory::from_runtime(&runtime, &quota, &controls, &live_work);

    let all = inventory
        .snapshots(&WorkerScope::all(), Utc::now())
        .unwrap();
    assert_eq!(all.len(), 2);
    assert!(
        !all.iter()
            .find(|account| account.engine == Engine::Claude)
            .unwrap()
            .binary_available
    );
    let routable = inventory
        .routing_snapshots(&WorkerScope::all(), Utc::now())
        .unwrap();
    assert_eq!(routable.len(), 1);
    assert_eq!(routable[0].engine, Engine::Kimi);
    assert_eq!(routable[0].profile, temp.path().join("custom/kimi-account"));
}

#[test]
fn a_later_binary_discovery_makes_the_same_pinned_profile_routable() {
    let temp = tempfile::tempdir().unwrap();
    let controls = AccountControlStore::new(
        temp.path().join("cooldown.json"),
        temp.path().join("paused.json"),
    );
    let quota = QuotaFixture {
        snapshots: BTreeMap::new(),
    };
    let live_work = LiveWorkFixture {
        snapshot: LiveWorkSnapshot::default(),
    };
    let unavailable = routing_runtime(temp.path(), false, false);
    let unavailable_inventory =
        AccountInventory::from_runtime(&unavailable, &quota, &controls, &live_work);
    assert!(unavailable_inventory
        .routing_snapshots(&WorkerScope::only(Engine::Claude), Utc::now())
        .unwrap()
        .is_empty());

    let available = routing_runtime(temp.path(), true, false);
    let available_inventory =
        AccountInventory::from_runtime(&available, &quota, &controls, &live_work);
    let routable = available_inventory
        .routing_snapshots(&WorkerScope::only(Engine::Claude), Utc::now())
        .unwrap();
    assert_eq!(routable.len(), 1);
    assert_eq!(routable[0].account, "orch");
    assert_eq!(
        routable[0].profile,
        temp.path().join("custom/claude-orchestrator")
    );
}
