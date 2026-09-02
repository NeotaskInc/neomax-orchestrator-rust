use std::collections::BTreeMap;

use crate::agent_tools::{
    LaunchRole, NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV, NEOMAX_TOOL_POLICY_ENV, ToolManifest, ToolPolicy,
};

#[test]
fn caller_policies_separate_command_classes() {
    let manifest = ToolManifest::canonical();
    assert!(
        ToolPolicy::read_only()
            .authorize(&manifest, "status")
            .is_ok()
    );
    assert!(
        ToolPolicy::read_only()
            .authorize(&manifest, "config set")
            .is_err()
    );
    assert!(
        ToolPolicy::worker()
            .authorize(&manifest, "config set")
            .is_ok()
    );
    assert!(
        ToolPolicy::worker()
            .authorize(&manifest, "dispatch")
            .is_err()
    );
    assert!(
        ToolPolicy::orchestrator()
            .authorize(&manifest, "dispatch")
            .is_ok()
    );
    assert!(
        ToolPolicy::orchestrator()
            .authorize(&manifest, "kill")
            .is_err()
    );
    assert!(ToolPolicy::full().authorize(&manifest, "kill").is_ok());
}

#[test]
fn unknown_commands_fail_closed() {
    let manifest = ToolManifest::canonical();
    assert!(
        ToolPolicy::full()
            .authorize(&manifest, "not-a-neomax-command")
            .is_err()
    );
}

#[test]
fn unknown_policy_names_fail_closed() {
    assert!(ToolPolicy::from_name("tampered").is_err());
    assert!(ToolPolicy::from_name("").is_err());
}

#[test]
fn full_policy_requires_an_explicit_opt_in() {
    assert!(ToolPolicy::from_name_with_full("full", false).is_err());
    let policy = ToolPolicy::from_name_with_full("full", true).unwrap();
    assert!(policy.is_full());
    assert_eq!(policy.as_name(), "full");
}

#[test]
fn environment_policy_defaults_to_the_launch_role() {
    let empty = BTreeMap::new();
    assert_eq!(
        ToolPolicy::from_environment(&empty, LaunchRole::Worker).unwrap(),
        ToolPolicy::worker()
    );
    assert_eq!(
        ToolPolicy::from_environment(&empty, LaunchRole::Orchestrator).unwrap(),
        ToolPolicy::orchestrator()
    );
}

#[test]
fn environment_policy_propagates_full_only_with_opt_in() {
    let mut values = BTreeMap::from([(NEOMAX_TOOL_POLICY_ENV.into(), "full".into())]);
    assert!(ToolPolicy::from_environment(&values, LaunchRole::Worker).is_err());
    values.insert(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(), "1".into());
    let policy = ToolPolicy::from_environment(&values, LaunchRole::Worker).unwrap();
    assert!(policy.is_full());
    assert!(policy.authorize(&ToolManifest::canonical(), "kill").is_ok());
}

#[test]
fn configured_role_mismatch_fails_closed() {
    let values = BTreeMap::from([(NEOMAX_TOOL_POLICY_ENV.into(), "orchestrator".into())]);
    assert!(ToolPolicy::from_environment(&values, LaunchRole::Worker).is_err());
}
