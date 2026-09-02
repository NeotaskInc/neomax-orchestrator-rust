use std::ffi::OsStr;
use std::path::PathBuf;

use crate::Engine;
use crate::config::WorkerScope;
use crate::providers::{OrchestratorEnvironment, OrchestratorRequest, ProviderProfile, catalog};

#[cfg(windows)]
pub(super) const HOME: &str = r"C:\neomax-fixture\home";
#[cfg(not(windows))]
pub(super) const HOME: &str = "/tmp/neomax-fixture/home";

#[cfg(windows)]
pub(super) const CWD: &str = r"C:\neomax-fixture\home\project";
#[cfg(not(windows))]
pub(super) const CWD: &str = "/tmp/neomax-fixture/test-home/project";

#[cfg(windows)]
const NEOMAX_BIN: &str = r"C:\neomax-fixture\home\bin\neomax.exe";
#[cfg(not(windows))]
const NEOMAX_BIN: &str = "/tmp/neomax-fixture/test-home/bin/neomax";

pub(super) fn request(engine: Engine) -> OrchestratorRequest {
    let profile = ProviderProfile {
        engine,
        account: "2".into(),
        path: PathBuf::from(HOME).join(catalog::spec(engine).default_profile_dir),
        reserved: false,
    };
    let request = OrchestratorRequest::new(
        profile,
        HOME,
        CWD,
        OrchestratorEnvironment::new(WorkerScope::all(), "orch-fixture")
            .with_pid(4242)
            .with_variable("NEOMAX_BIN", NEOMAX_BIN)
            .with_variable("NEOMAX_TOOL_POLICY", "orchestrator"),
    );
    if engine == Engine::Kimi {
        request.with_agent_file(PathBuf::from(HOME).join("kimi-agent.md"))
    } else {
        request
    }
}

pub(super) fn args(engine: Engine, request: &OrchestratorRequest) -> Vec<String> {
    super::super::build(engine, OsStr::new(engine.as_str()), request)
        .unwrap()
        .args_lossy()
}
