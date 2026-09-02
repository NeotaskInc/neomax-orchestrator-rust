use crate::Engine;
use crate::agent_tools::{
    ExecutableInputs, NEOMAX_TOOL_DEPTH_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV, RecursionGuard,
};
use crate::runs::execution::tooling::{WorkerToolingInput, prepare_worker_tools};

use super::fixtures::{executable, paths, request, settings};

#[test]
fn nested_worker_depth_advances_and_stops_at_the_configured_limit() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let settings = settings();

    let root_request = request(Engine::Grok, temp.path());
    let root = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings,
        request: &root_request,
        executable_inputs: ExecutableInputs::new(Some(binary.clone()), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();
    assert_eq!(
        root.variables().get(NEOMAX_TOOL_DEPTH_ENV),
        Some(&"1".into())
    );

    let mut child_request = request(Engine::Claude, temp.path());
    child_request
        .agent_environment
        .extend(root.variables().clone());
    child_request
        .agent_environment
        .insert(NEOMAX_TOOL_MAX_DEPTH_ENV.into(), "2".into());
    let child = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings,
        request: &child_request,
        executable_inputs: ExecutableInputs::new(Some(binary.clone()), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();
    assert_eq!(
        child.variables().get(NEOMAX_TOOL_DEPTH_ENV),
        Some(&"2".into())
    );

    let mut blocked_request = request(Engine::Codex, temp.path());
    blocked_request
        .agent_environment
        .extend(child.variables().clone());
    assert!(
        prepare_worker_tools(WorkerToolingInput {
            paths: &paths,
            settings: &settings,
            request: &blocked_request,
            executable_inputs: ExecutableInputs::new(Some(binary), None),
            ambient_path: None,
            inherited_depth: None,
            inherited_max_depth: None,
        })
        .is_err()
    );
}

#[test]
fn malformed_depth_values_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let settings = settings();
    let mut request = request(Engine::Opencode, temp.path());
    request
        .agent_environment
        .insert(NEOMAX_TOOL_DEPTH_ENV.into(), "not-a-number".into());

    assert!(
        prepare_worker_tools(WorkerToolingInput {
            paths: &paths,
            settings: &settings,
            request: &request,
            executable_inputs: ExecutableInputs::new(Some(binary), None),
            ambient_path: None,
            inherited_depth: None,
            inherited_max_depth: None,
        })
        .is_err()
    );
    assert!(RecursionGuard::new(0, 0).is_err());
}
