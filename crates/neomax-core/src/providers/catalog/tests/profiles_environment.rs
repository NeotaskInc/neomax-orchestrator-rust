use crate::providers::catalog::{
    discover_profile_snapshots, AuthMethod, AuthStatus, MapEnvironment,
};
use crate::Engine;

use super::super::fixtures;
#[test]
fn environment_api_keys_authenticate_only_the_effective_default_profile() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let fs = fixtures::FixtureFs::default()
        .dir(home.join(".claude"))
        .dir(home.join(".claude-acct2"));
    let environment = MapEnvironment::new([("ANTHROPIC_API_KEY".into(), "fixture-token".into())])
        .with_home(home)
        .with_current_dir(home);
    let profiles = discover_profile_snapshots(Engine::Claude, &environment, &fs).unwrap();
    let default = profiles
        .iter()
        .find(|profile| profile.path == home.join(".claude"))
        .unwrap();
    assert!(default.eligibility.authenticated);
    assert!(default.eligibility.worker_eligible);
    assert!(!default.eligibility.rotation_eligible);
    assert!(matches!(
        default.auth,
        AuthStatus::Authenticated { ref methods } if methods == &[AuthMethod::ApiKey]
    ));
    let extra = profiles
        .iter()
        .find(|profile| profile.path == home.join(".claude-acct2"))
        .unwrap();
    assert!(!extra.eligibility.authenticated);
    assert!(!format!("{profiles:?}").contains("fixture-token"));
}
