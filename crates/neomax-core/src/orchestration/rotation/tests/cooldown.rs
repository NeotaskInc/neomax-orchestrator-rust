use std::fs;

use serde_json::json;

use crate::atomic::{read_json, write_json_atomic};

use super::*;

#[test]
fn account_cooldowns_are_uuid_keyed_and_preserve_unknown_entries() {
    let temp = tempfile::tempdir().unwrap();
    let store = AccountCooldownStore::in_state_dir(temp.path());
    write_json_atomic(
        store.path(),
        &json!({"uuid-a": 100, "future": {"keep": true}}),
    )
    .unwrap();
    store.set("uuid-b", 200).unwrap();
    assert!(store.is_cooled("uuid-b", 199));
    assert!(!store.is_cooled("uuid-b", 200));
    assert_eq!(store.cooldown_until("uuid-a", 99), Some(100));
    assert_eq!(store.cooldown_until("", 0), None);
    let raw: serde_json::Value = read_json(store.path()).unwrap();
    assert_eq!(raw["future"]["keep"], true);
    assert!(store.clear("uuid-b").unwrap());
    assert!(!store.clear("uuid-b").unwrap());
}

#[test]
fn malformed_cooldown_state_is_safe() {
    let temp = tempfile::tempdir().unwrap();
    let store = AccountCooldownStore::in_state_dir(temp.path());
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(store.path(), b"not-json").unwrap();
    assert!(store.cooldowns().is_empty());
    assert!(!store.clear("uuid").unwrap());
    assert_eq!(fs::read(store.path()).unwrap(), b"not-json");
    store.set("uuid", 10).unwrap();
    assert_eq!(store.cooldown_until("uuid", 0), Some(10));
}
