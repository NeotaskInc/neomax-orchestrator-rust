use std::collections::BTreeMap;
use std::ffi::OsStr;

use crate::Engine;
use crate::agent_tools::{
    ExecutableInputs, LaunchRole, NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV, NEOMAX_BIN_ENV,
    NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_POLICY_ENV, ToolPolicy,
};
use crate::runs::execution::tooling::{
    WorkerToolingInput, prepare_worker_tools, resolve_policy_for_test,
};

use super::fixtures::{executable, paths, request, settings};

#[test]
fn worker_environment_augments_the_supplied_path_without_replacing_it() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax-current");
    let ambient_entries = [
        temp.path().join("ambient-one"),
        temp.path().join("ambient-two"),
    ];
    let ambient_path = std::env::join_paths(ambient_entries.iter()).unwrap();
    let paths = paths(temp.path());
    let request = request(Engine::Claude, temp.path());
    let prepared = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary.clone()), None),
        ambient_path: Some(ambient_path),
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();

    let variables = prepared.variables();
    let canonical_binary = std::fs::canonicalize(binary).unwrap();
    assert_eq!(
        variables.get(NEOMAX_BIN_ENV),
        Some(&canonical_binary.to_string_lossy().into())
    );
    assert!(variables.contains_key(NEOMAX_TOOL_MANIFEST_ENV));
    assert_eq!(
        variables.get(NEOMAX_TOOL_POLICY_ENV),
        Some(&LaunchRole::Worker.policy_name().to_string())
    );
    let path = variables.get("PATH").unwrap();
    let path_entries = std::env::split_paths(OsStr::new(path)).collect::<Vec<_>>();
    assert!(path_entries.contains(&canonical_binary.parent().unwrap().to_path_buf()));
    assert!(path_entries.contains(&ambient_entries[0]));
    assert!(path_entries.contains(&ambient_entries[1]));
    assert_eq!(variables.get("NEOMAX_MAX_SUBAGENTS"), None);
}

#[test]
fn worker_launch_rederives_policy_from_an_inherited_orchestrator_environment() {
    let policy = resolve_policy_for_test(
        &BTreeMap::new(),
        Some("orchestrator"),
        None,
        LaunchRole::Worker,
    )
    .unwrap();
    assert_eq!(policy, ToolPolicy::worker());
}

#[test]
fn explicit_request_policy_mismatch_is_rejected() {
    let configured = BTreeMap::from([(NEOMAX_TOOL_POLICY_ENV.into(), "orchestrator".into())]);
    let result =
        resolve_policy_for_test(&configured, Some("orchestrator"), None, LaunchRole::Worker);
    assert!(result.is_err());
}

#[test]
fn ambient_full_policy_requires_opt_in_but_is_preserved_when_opted_in() {
    let denied = resolve_policy_for_test(&BTreeMap::new(), Some("full"), None, LaunchRole::Worker);
    assert!(denied.is_err());

    let allowed = resolve_policy_for_test(
        &BTreeMap::new(),
        Some("full"),
        Some("1"),
        LaunchRole::Worker,
    )
    .unwrap();
    assert_eq!(allowed, ToolPolicy::full());
}

#[test]
fn request_path_overrides_the_ambient_path_only_for_path_composition() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax-current");
    let request_bin = temp.path().join("request-bin");
    let ambient_bin = temp.path().join("ambient-bin");
    let request_path = std::env::join_paths([request_bin.clone()]).unwrap();
    let ambient_path = std::env::join_paths([ambient_bin.clone()]).unwrap();
    let paths = paths(temp.path());
    let mut request = request(Engine::Codex, temp.path());
    request
        .agent_environment
        .insert("PATH".into(), request_path.to_string_lossy().into_owned());
    let prepared = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary), None),
        ambient_path: Some(ambient_path),
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();

    let path = prepared.variables().get("PATH").unwrap();
    let path_entries = std::env::split_paths(OsStr::new(path)).collect::<Vec<_>>();
    assert!(path_entries.contains(&request_bin));
    assert!(!path_entries.contains(&ambient_bin));
}

#[test]
fn explicit_full_policy_is_validated_and_propagated_to_the_child() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax-current");
    let paths = paths(temp.path());
    let mut request = request(Engine::Opencode, temp.path());
    request
        .agent_environment
        .insert(NEOMAX_TOOL_POLICY_ENV.into(), "full".into());
    request
        .agent_environment
        .insert(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(), "true".into());
    let prepared = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();

    assert_eq!(prepared.policy(), ToolPolicy::full());
    assert_eq!(
        prepared.variables().get(NEOMAX_TOOL_POLICY_ENV),
        Some(&"full".to_string())
    );
    assert_eq!(
        prepared.variables().get(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV),
        Some(&"1".to_string())
    );
}

#[test]
fn unopted_full_policy_is_rejected_before_provider_launch() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax-current");
    let paths = paths(temp.path());
    let mut request = request(Engine::Grok, temp.path());
    request
        .agent_environment
        .insert(NEOMAX_TOOL_POLICY_ENV.into(), "full".into());
    let result = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    });
    assert!(result.is_err());
}
