use crate::Engine;
use crate::agent_tools::ExecutableInputs;
use crate::runs::execution::tooling::{WorkerToolingInput, prepare_worker_tools};

use super::fixtures::{executable, paths, request, settings};

#[test]
fn all_provider_engines_receive_the_same_tool_contract() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let settings = settings();
    let mut environments = Vec::new();

    for engine in Engine::ALL {
        let request = request(engine, temp.path());
        let prepared = prepare_worker_tools(WorkerToolingInput {
            paths: &paths,
            settings: &settings,
            request: &request,
            executable_inputs: ExecutableInputs::new(Some(binary.clone()), None),
            ambient_path: None,
            inherited_depth: None,
            inherited_max_depth: None,
        })
        .unwrap();
        environments.push(prepared.variables().clone());
    }

    for environment in environments.iter().skip(1) {
        assert_eq!(environment, &environments[0]);
    }
}
