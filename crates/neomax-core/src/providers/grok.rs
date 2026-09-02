use std::ffi::{OsStr, OsString};

use crate::providers::catalog::GROK_DEFAULT_MODEL;
use crate::providers::worker::{apply_profile, base_command, composed_prompt};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events,
};
use crate::{Engine, Result};

pub struct Grok {
    binary: OsString,
}

impl Grok {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for Grok {
    fn engine(&self) -> Engine {
        Engine::Grok
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        GROK_DEFAULT_MODEL
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        catalog::current_profiles(self.engine())
    }

    fn auth_state(&self, profile: &ProviderProfile) -> AuthState {
        auth::current_auth_state(profile)
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> Result<ProviderCommand> {
        let request = context.request();
        let mut command = base_command(&self.binary, context)
            .arg("--no-auto-update")
            .arg("--output-format")
            .arg("streaming-json")
            .arg("--cwd")
            .arg(request.cwd.as_os_str())
            .arg("--model")
            .arg(request.selected_model());
        command = if request.plan {
            command.arg("--permission-mode").arg("plan")
        } else {
            command.arg("--always-approve")
        };
        if let Some(session) = request.resume_session.as_deref() {
            command = command.arg("--resume").arg(session);
        }
        if let Some(turns) = request.max_turns {
            command = command.arg("--max-turns").arg(turns.to_string());
        }
        command = command
            .arg("--single")
            .arg(composed_prompt(context, None))
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0");
        apply_profile(command, request, None)
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_grok(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn pins_model_resume_turns_approval_and_goal() {
        let profile = ProviderProfile {
            engine: Engine::Grok,
            account: "2".into(),
            path: PathBuf::from("/profiles/grok-2"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.resume_session = Some("session_resume".into());
        request.goal = Some("tests pass".into());
        request.max_turns = Some(4);
        let context = WorkerLaunchContext::for_test(request);
        let args = Grok::new("grok")
            .worker_command(&context)
            .unwrap()
            .args_lossy();
        assert!(args.contains(&GROK_DEFAULT_MODEL.into()));
        assert!(args.contains(&"--always-approve".into()));
        assert!(args.contains(&"session_resume".into()));
        assert!(args.last().unwrap().contains("OBJECTIVE"));
    }
}
