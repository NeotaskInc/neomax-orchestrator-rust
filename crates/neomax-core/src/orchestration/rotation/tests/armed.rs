use std::fs;
use std::sync::{Arc, Barrier};

use serde_json::json;

use crate::atomic::{read_json, write_json_atomic};

use super::*;

#[test]
fn armed_markers_preserve_human_directives_and_unknown_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());
    let profile = temp.path().join("profiles/../profile-a");
    let key = normalize_profile_path(&profile)
        .to_string_lossy()
        .into_owned();
    let mut initial = serde_json::Map::new();
    initial.insert(
        key.clone(),
        json!({
            "threshold": 90.0,
            "weekly_threshold": 95.0,
            "prefer": ["2"],
            "auto": false,
            "session": null,
            "ts": 100,
            "future": {"keep": true}
        }),
    );
    initial.insert("/other/profile".into(), json!({"future": "untouched"}));
    write_json_atomic(store.path(), &serde_json::Value::Object(initial)).unwrap();

    let auto = store.arm(&profile, 99.0, 99.0, &[], true, 101).unwrap();
    assert!(!auto.auto);
    assert_eq!(auto.threshold, 90.0);
    assert_eq!(auto.prefer, vec!["2"]);

    let claim = store
        .claim(&profile, Some("session-1"), 102)
        .unwrap()
        .unwrap();
    assert_eq!(claim.threshold, 90.0);
    assert_eq!(claim.weekly_threshold, 95.0);
    assert_eq!(claim.prefer, Some(vec!["2".into()]));
    assert!(
        store
            .claim(&profile, Some("session-2"), 103)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .refresh(&profile, Some("session-1"), 104)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .claim(
                &profile,
                Some("session-1"),
                104 + ARMED_ROTATE_AGE_SECONDS + 1
            )
            .unwrap()
            .is_none()
    );

    let raw: serde_json::Value = read_json(store.path()).unwrap();
    assert_eq!(raw[&key]["future"]["keep"], true);
    assert_eq!(raw["/other/profile"]["future"], "untouched");
    assert!(store.clear(&profile).unwrap());
    assert!(!store.clear(&profile).unwrap());
}

#[test]
fn malformed_armed_state_is_safe_and_recoverable() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(store.path(), b"{").unwrap();
    assert!(store.records().is_empty());
    assert!(store.claim("profile", Some("s1"), 10).unwrap().is_none());
    let record = store.arm("profile", 99.0, 99.0, &[], true, 10).unwrap();
    assert!(record.auto);
    assert!(store.claim("profile", Some("s1"), 11).unwrap().is_some());
}

#[test]
fn failed_armed_mutations_preserve_corrupt_optional_state() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());
    fs::create_dir_all(temp.path()).unwrap();
    fs::write(store.path(), b"{").unwrap();
    assert!(store.claim("profile", Some("s1"), 10).unwrap().is_none());
    assert_eq!(fs::read(store.path()).unwrap(), b"{");
    assert!(!store.clear("profile").unwrap());
    assert_eq!(fs::read(store.path()).unwrap(), b"{");
}

#[test]
fn canonicalizes_legacy_relative_armed_keys() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());
    let mut initial = serde_json::Map::new();
    initial.insert(
        "./profile".into(),
        json!({"threshold": 90.0, "weekly_threshold": 95.0, "auto": false, "ts": 100}),
    );
    write_json_atomic(store.path(), &serde_json::Value::Object(initial)).unwrap();

    assert_eq!(store.get("profile").unwrap().threshold, 90.0);
    store.arm("profile", 99.0, 99.0, &[], true, 101).unwrap();
    let raw: serde_json::Value = read_json(store.path()).unwrap();
    let canonical = normalize_profile_path("profile")
        .to_string_lossy()
        .into_owned();
    assert!(raw.get("./profile").is_none());
    assert_eq!(raw[&canonical]["threshold"], 90.0);
}

#[test]
fn armed_claims_are_atomic_and_single_owner() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());
    store.arm("profile", 99.0, 99.0, &[], true, 100).unwrap();
    let store = Arc::new(store);
    let barrier = Arc::new(Barrier::new(2));
    let profile = std::path::PathBuf::from("profile");

    let (first, second) = std::thread::scope(|scope| {
        let first_store = Arc::clone(&store);
        let first_barrier = Arc::clone(&barrier);
        let first_profile = profile.clone();
        let first = scope.spawn(move || {
            first_barrier.wait();
            first_store.claim(first_profile, Some("s1"), 101).unwrap()
        });
        let second_store = Arc::clone(&store);
        let second_barrier = Arc::clone(&barrier);
        let second = scope.spawn(move || {
            second_barrier.wait();
            second_store.claim(profile, Some("s2"), 101).unwrap()
        });
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(first.is_some() as u8 + second.is_some() as u8, 1);
}

#[cfg(windows)]
#[test]
fn rejects_windows_partial_root_profiles_before_persisting_armed_state() {
    let temp = tempfile::tempdir().unwrap();
    let store = ArmedRotateStore::in_state_dir(temp.path());

    for raw in [r"\rooted", r"C:drive-relative"] {
        let profile = std::path::Path::new(raw);
        assert!(store.arm(profile, 99.0, 99.0, &[], true, 10).is_err());
        assert!(store.clear(profile).is_err());
        assert!(store.claim(profile, Some("session"), 10).is_err());
        assert!(store.refresh(profile, Some("session"), 10).is_err());
    }
    assert!(!store.path().exists());

    write_json_atomic(
        store.path(),
        &serde_json::json!({
            r"\rooted": {"auto": true, "ts": 10},
            "C:drive-relative": {"auto": true, "ts": 10}
        }),
    )
    .unwrap();
    assert!(store.records().is_empty());
}
