use std::ffi::OsString;
use std::fs;

use crate::Engine;
use crate::agent_tools::{ExecutableInputs, NEOMAX_BIN_ENV, ToolManifest, ToolPolicy};
use crate::runs::execution::tooling::{WorkerToolingInput, prepare_worker_tools};

use super::fixtures::{executable, paths, request, settings};

#[test]
fn executable_resolution_does_not_fall_back_to_ambient_path() {
    let temp = tempfile::tempdir().unwrap();
    let paths = paths(temp.path());
    let request = request(Engine::Claude, temp.path());
    let result = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(None, None),
        ambient_path: Some(OsString::from(temp.path())),
        inherited_depth: None,
        inherited_max_depth: None,
    });
    assert!(result.is_err());
}

#[test]
fn explicit_install_bin_is_used_when_current_executable_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let installed = executable(temp.path(), "installed-neomax");
    let paths = paths(temp.path());
    let request = request(Engine::Kimi, temp.path());
    let prepared = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(
            Some(temp.path().join("missing-neomax")),
            Some(installed.clone()),
        ),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();
    let resolved = fs::canonicalize(installed).unwrap();
    assert_eq!(
        prepared.variables().get(NEOMAX_BIN_ENV),
        Some(&resolved.to_string_lossy().into())
    );
}

#[test]
fn caller_environment_secrets_are_not_copied_into_tool_variables() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let mut request = request(Engine::Grok, temp.path());
    request
        .agent_environment
        .insert("PROVIDER_API_KEY".into(), "fixture-secret".into());
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
    assert!(!prepared.variables().contains_key("PROVIDER_API_KEY"));
    assert!(
        !prepared
            .variables()
            .values()
            .any(|value| value.contains("fixture-secret"))
    );
    assert!(prepared.variables().contains_key(NEOMAX_BIN_ENV));
}

#[test]
fn worker_permissions_allow_project_mutations_but_deny_dispatch_and_destruction() {
    let manifest = ToolManifest::canonical();
    let policy = ToolPolicy::worker();
    assert!(policy.authorize(&manifest, "status").is_ok());
    assert!(policy.authorize(&manifest, "config set").is_ok());
    assert!(policy.authorize(&manifest, "dispatch").is_err());
    assert!(policy.authorize(&manifest, "clean").is_err());
}
