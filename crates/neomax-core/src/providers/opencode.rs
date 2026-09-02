use std::ffi::{OsStr, OsString};

use crate::providers::catalog::OPENCODE_DEFAULT_MODEL;
use crate::providers::worker::{apply_profile, base_command, composed_prompt};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events, opencode_policy,
};
use crate::{Engine, Result};

pub struct OpenCode {
    binary: OsString,
}

impl OpenCode {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for OpenCode {
    fn engine(&self) -> Engine {
        Engine::Opencode
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        OPENCODE_DEFAULT_MODEL
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        catalog::current_profiles(self.engine())
    }

    fn auth_state(&self, profile: &ProviderProfile) -> AuthState {
        auth::current_auth_state(profile)
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> Result<ProviderCommand> {
        let request = context.request();
        let model = request.selected_model();
        let policy = opencode_policy::content(model)?;
        let mut command = base_command(&self.binary, context)
            .arg("run")
            .arg("--model")
            .arg(model)
            .arg("--format")
            .arg("json")
            .arg("--dir")
            .arg(request.cwd.as_os_str())
            .arg("--agent")
            .arg(if request.plan { "plan" } else { "build" });
        if !request.plan {
            command = command.arg("--auto");
        }
        if let Some(session) = request.resume_session.as_deref() {
            command = command.arg("--session").arg(session);
        }
        command = command
            .arg(composed_prompt(context, None))
            .env("OPENCODE_CONFIG_CONTENT", policy)
            .env("OPENCODE_DISABLE_AUTOUPDATE", "1")
            .env("OPENCODE_DISABLE_SHARE", "1")
            .env("OPENCODE_AUTO_SHARE", "false")
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0");
        apply_profile(command, request, None)
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_opencode(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn pins_registry_model_policy_and_resume_session() {
        let profile = ProviderProfile {
            engine: Engine::Opencode,
            account: "2".into(),
            path: PathBuf::from("/profiles/opencode-2"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.model = Some("opencode/big-pickle".into());
        request.resume_session = Some("session_resume".into());
        request.goal = Some("tests pass".into());
        let context = WorkerLaunchContext::for_test(request);
        let command = OpenCode::new("opencode").worker_command(&context).unwrap();
        let args = command.args_lossy();
        assert_eq!(&args[..2], ["run", "--model"]);
        assert!(args.contains(&"opencode/big-pickle".into()));
        assert!(args.contains(&"session_resume".into()));
        let policy: serde_json::Value = serde_json::from_str(
            command
                .env
                .get(OsStr::new("OPENCODE_CONFIG_CONTENT"))
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(policy["model"], "opencode/big-pickle");
        assert_eq!(
            command.env.get(OsStr::new("OPENCODE_AUTO_SHARE")),
            Some(&OsString::from("false"))
        );
    }
}
