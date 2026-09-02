use neomax_core::Engine;
use neomax_core::providers::ProviderProfile;

use super::super::request::AccountSelector;
use super::{DetectedAuth, ManagedProfile, profile_for, profile_path};

#[test]
fn profile_paths_are_provider_specific_and_portable() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    assert_eq!(
        profile_path(Engine::Codex, &AccountSelector::Number(1), &home).unwrap(),
        home.join(".codex")
    );
    assert_eq!(
        profile_path(Engine::Kimi, &AccountSelector::Number(2), &home).unwrap(),
        home.join(".kimi-code-acct2")
    );
    assert_eq!(
        profile_path(Engine::Grok, &AccountSelector::Orchestrator, &home).unwrap(),
        home.join(".grok-orch")
    );
}

#[test]
fn profile_selection_requires_an_existing_profile() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ManagedProfile {
        profile: ProviderProfile {
            engine: Engine::Kimi,
            account: "2".into(),
            path: temp.path().join("profile"),
            reserved: false,
        },
        auth: Some(DetectedAuth::OAuth),
    };
    assert_eq!(
        profile_for(&[profile], &AccountSelector::Number(2))
            .unwrap()
            .account(),
        "2"
    );
    assert!(profile_for(&[], &AccountSelector::Number(1)).is_err());
}
