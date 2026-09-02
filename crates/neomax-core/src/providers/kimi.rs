use std::ffi::{OsStr, OsString};

use crate::Error;
use crate::providers::catalog::KIMI_DEFAULT_MODEL;
use crate::providers::worker::{apply_profile, base_command, composed_prompt};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events,
};
use crate::{Engine, Result};

const PLAN_NOTE: &str = "READ-ONLY PLAN SCOUT: inspect and report a plan only. The runtime exposes read-only tools; do not attempt edits, writes, shell commands, or external mutations.";

pub struct Kimi {
    binary: OsString,
}

impl Kimi {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for Kimi {
    fn engine(&self) -> Engine {
        Engine::Kimi
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        KIMI_DEFAULT_MODEL
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        catalog::current_profiles(self.engine())
    }

    fn auth_state(&self, profile: &ProviderProfile) -> AuthState {
        auth::current_auth_state(profile)
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> Result<ProviderCommand> {
        let request = context.request();
        if request.plan && request.config_home_override.is_none() {
            return Err(Error::InvalidArgument(
                "Kimi plan mode requires a prepared read-only config home".into(),
            ));
        }
        let mut command = base_command(&self.binary, context)
            .arg("-m")
            .arg(request.selected_model())
            .arg("--output-format")
            .arg("stream-json");
        if let Some(session) = request.resume_session.as_deref() {
            command = command.arg("-S").arg(session);
        }
        command = command
            .arg("--prompt")
            .arg(composed_prompt(context, request.plan.then_some(PLAN_NOTE)))
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0");
        apply_profile(command, request, request.config_home_override.as_ref())
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_kimi(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn supports_resume_goal_and_read_only_planning_prompt() {
        let profile = ProviderProfile {
            engine: Engine::Kimi,
            account: "2".into(),
            path: PathBuf::from("/profiles/kimi-2"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.resume_session = Some("session_resume".into());
        request.goal = Some("tests pass".into());
        request.plan = true;
        let temp = tempfile::tempdir().unwrap();
        request.config_home_override = Some(temp.path().to_path_buf());
        let context = WorkerLaunchContext::for_test(request);
        let args = Kimi::new("kimi")
            .worker_command(&context)
            .unwrap()
            .args_lossy();
        assert_eq!(
            args[args.iter().position(|item| item == "-m").unwrap() + 1],
            KIMI_DEFAULT_MODEL
        );
        assert!(args.contains(&"session_resume".into()));
        assert!(args.last().unwrap().contains("READ-ONLY PLAN SCOUT"));
        assert!(!args.contains(&"--auto".into()));
    }
}
