use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use crate::providers::{
    AuthState, OrchestratorRequest, ParsedEvents, Provider, ProviderCommand, ProviderProfile,
};
use crate::runs::RunRecord;
use crate::{Engine, Error, Result};

use super::super::SupervisorConfig;

pub(super) struct FakeProvider;

impl Provider for FakeProvider {
    fn engine(&self) -> Engine {
        Engine::Claude
    }

    fn binary(&self) -> &OsStr {
        OsStr::new("/bin/sh")
    }

    fn default_model(&self) -> &str {
        "model"
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        Ok(Vec::new())
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Unknown
    }

    fn worker_command(
        &self,
        _context: &crate::providers::WorkerLaunchContext,
    ) -> Result<ProviderCommand> {
        Err(Error::Message("not used".into()))
    }

    fn orchestrator_command(&self, request: &OrchestratorRequest) -> Result<ProviderCommand> {
        let session = request.session_id.as_deref().unwrap_or("missing");
        Ok(ProviderCommand::new("/bin/sh", &request.cwd)
            .arg("-c")
            .arg(format!("printf 'ok-resumed-{session}'")))
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        let text = String::from_utf8_lossy(bytes);
        Ok(ParsedEvents {
            result_text: text.contains("ok").then(|| "complete".into()),
            session_id: text
                .contains("resume_hint")
                .then(|| "session-bootstrap".into()),
            rate_limited: text.contains("rate"),
            ..ParsedEvents::default()
        })
    }
}

pub(super) fn command(root: &Path, script: &str) -> ProviderCommand {
    ProviderCommand::new("/bin/sh", root).arg("-c").arg(script)
}

pub(super) fn run(root: &Path) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id":"run", "engine":"claude", "model":"model", "prompt":"work",
        "profile":"/profiles/.claude1", "workdir":root, "status":"running", "started":1
    }))
    .unwrap()
}

pub(super) fn config() -> SupervisorConfig {
    SupervisorConfig {
        wall_timeout: Some(Duration::from_secs(2)),
        stall_timeout: Some(Duration::from_secs(2)),
        poll_interval: Duration::from_millis(10),
        terminate_grace: Duration::from_millis(20),
    }
}
