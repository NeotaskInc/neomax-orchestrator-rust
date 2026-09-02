use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::super::{AccountId, HandoffBaton, HandoffStore};
use crate::Engine;

fn baton() -> HandoffBaton {
    HandoffBaton {
        ts: 1_725_000_000,
        engine: Engine::Opencode,
        from_account: AccountId::from("2"),
        to_account: Some(AccountId::from("3")),
        reason: "manual /rotate".into(),
        cwd: PathBuf::from("/workspace/project"),
        five_hour: None,
        seven_day: Some(92.5),
        extra: BTreeMap::new(),
    }
}

#[test]
fn persists_the_compatible_baton_shape_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let store = HandoffStore::at_state_dir(temp.path());
    let expected = baton();
    store.save(&expected).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded, expected);
    let json: serde_json::Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(json["engine"], "opencode");
    assert_eq!(json["from_account"], 2);
    assert_eq!(json["to_account"], 3);
    assert_eq!(json["cwd"], "/workspace/project");
    assert!(json.get("five_hour").is_none());
    assert!(json.get("seven_day").is_some());
    assert!(store.lock_path().exists());
}

#[test]
fn reads_legacy_numeric_quota_fields_and_omits_unknown_values() {
    let legacy: HandoffBaton = serde_json::from_value(serde_json::json!({
        "ts": 1,
        "engine": "claude",
        "from_account": 1,
        "to_account": 2,
        "reason": "quota",
        "cwd": "/workspace",
        "five_hour": 99.0,
        "seven_day": 42.0
    }))
    .unwrap();
    assert_eq!(legacy.five_hour, Some(99.0));
    assert_eq!(legacy.seven_day, Some(42.0));

    let unknown = HandoffBaton {
        five_hour: None,
        seven_day: None,
        ..legacy
    };
    let json = serde_json::to_value(unknown).unwrap();
    assert!(json.get("five_hour").is_none());
    assert!(json.get("seven_day").is_none());
}

#[test]
fn missing_baton_is_not_an_error_and_clear_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let store = HandoffStore::at_state_dir(temp.path());
    assert_eq!(store.load().unwrap(), None);
    assert!(!store.clear().unwrap());
    store.save(&baton()).unwrap();
    assert!(store.clear().unwrap());
    assert!(!store.clear().unwrap());
}

#[test]
fn preserves_the_orchestrator_label_and_reads_legacy_numeric_accounts() {
    let reserved = AccountId::from("orch");
    assert_eq!(serde_json::to_value(&reserved).unwrap(), "orch");
    let numeric: AccountId = serde_json::from_value(serde_json::json!(7)).unwrap();
    assert_eq!(numeric.as_str(), "7");
    let legacy: AccountId = serde_json::from_value(serde_json::json!("8")).unwrap();
    assert_eq!(legacy.as_str(), "8");
}

#[test]
fn concurrent_writes_leave_valid_complete_json() {
    let temp = tempfile::tempdir().unwrap();
    let store = HandoffStore::at_state_dir(temp.path());
    std::thread::scope(|scope| {
        for index in 0..8 {
            let store = &store;
            scope.spawn(move || {
                let mut value = baton();
                value.ts = index;
                value.from_account = AccountId::from(index.to_string());
                store.save(&value).unwrap();
            });
        }
    });
    let loaded = store.load().unwrap().unwrap();
    assert!(loaded.ts < 8);
    assert!(!loaded.from_account.as_str().is_empty());
}
