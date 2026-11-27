use std::ffi::{OsStr, OsString};

use crate::providers::catalog::{ANTIGRAVITY_DEFAULT_EFFORT, ANTIGRAVITY_DEFAULT_MODEL};
use crate::providers::worker::{apply_profile, base_command, composed_prompt};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events,
};
use crate::{Engine, Result};

pub(crate) const AUTO_UPDATE_ENV: &str = "AGY_CLI_DISABLE_AUTO_UPDATE";

pub struct Antigravity {
    binary: OsString,
}

impl Antigravity {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for Antigravity {
    fn engine(&self) -> Engine {
        Engine::Antigravity
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        ANTIGRAVITY_DEFAULT_MODEL
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
            .arg("--output-format")
            .arg("stream-json")
            .arg("--model")
            .arg(request.selected_model())
            .arg("--effort")
            .arg(
                request
                    .effort
                    .as_deref()
                    .or_else(|| request.ultra.then_some("xhigh"))
                    .unwrap_or(ANTIGRAVITY_DEFAULT_EFFORT),
            );
        command = if request.plan {
            command.arg("--mode").arg("plan")
        } else {
            command.arg("--dangerously-skip-permissions")
        };
        if let Some(conversation) = request.resume_session.as_deref() {
            command = command.arg("--conversation").arg(conversation);
        }
        command = command
            .arg("--print")
            .arg(composed_prompt(context, None))
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env(AUTO_UPDATE_ENV, "true");
        apply_profile(command, request, None)
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_antigravity(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn pins_neon_at_high_effort_and_resumes_the_conversation() {
        let profile = ProviderProfile {
            engine: Engine::Antigravity,
            account: "1".into(),
            path: PathBuf::from("/profiles/gemini"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.resume_session = Some("conversation-1".into());
        let context = WorkerLaunchContext::for_test(request);
        let command = Antigravity::new("agy").worker_command(&context).unwrap();
        let args = command.args_lossy();
        let value = |flag: &str| {
            args.iter()
                .position(|argument| argument == flag)
                .and_then(|index| args.get(index + 1))
                .cloned()
        };
        assert_eq!(value("--model").as_deref(), Some(ANTIGRAVITY_DEFAULT_MODEL));
        assert_eq!(value("--effort").as_deref(), Some("high"));
        assert_eq!(value("--conversation").as_deref(), Some("conversation-1"));
        assert_eq!(value("--output-format").as_deref(), Some("stream-json"));
        assert!(args.contains(&"--dangerously-skip-permissions".into()));
        assert!(value("--print").unwrap().ends_with("Do the work."));
    }
}
