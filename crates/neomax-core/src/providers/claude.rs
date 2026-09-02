use std::ffi::{OsStr, OsString};

use crate::providers::catalog::CLAUDE_DEFAULT_MODEL;
use crate::providers::worker::{DIRECTIVE, ORCHESTRATOR_DIRECTIVE, apply_profile, base_command};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events,
};
use crate::{Engine, Result};

pub struct Claude {
    binary: OsString,
}

impl Claude {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for Claude {
    fn engine(&self) -> Engine {
        Engine::Claude
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        CLAUDE_DEFAULT_MODEL
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        catalog::current_profiles(self.engine())
    }

    fn auth_state(&self, profile: &ProviderProfile) -> AuthState {
        auth::current_auth_state(profile)
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> Result<ProviderCommand> {
        let request = context.request();
        let mut command = base_command(&self.binary, context).arg("-p");
        command = if request.plan {
            command.arg("--permission-mode").arg("plan")
        } else {
            command.arg("--dangerously-skip-permissions")
        };
        command = command
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages")
            .arg("--append-system-prompt")
            .arg(if context.role().is_orchestrator() {
                ORCHESTRATOR_DIRECTIVE
            } else {
                DIRECTIVE
            });
        if request.ultra {
            command = command
                .arg("--settings")
                .arg(serde_json::json!({"ultracode": true}).to_string());
        }
        command = command.arg("--model").arg(request.selected_model());
        if let Some(turns) = request.max_turns {
            command = command.arg("--max-turns").arg(turns.to_string());
        }
        if let Some(effort) = request.effort.as_deref() {
            command = command.arg("--effort").arg(effort);
        }
        if let Some(session) = request.resume_session.as_deref() {
            command = command.arg("--resume").arg(session);
        } else if let Some(session) = request.session_id.as_deref() {
            command = command.arg("--session-id").arg(session);
        }
        let prompt = request.goal.as_ref().map_or_else(
            || request.prompt.clone(),
            |goal| format!("/goal {goal}\n\n{}", request.prompt),
        );
        apply_profile(command.arg(prompt), request, None)
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_claude(bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn constructs_the_full_headless_worker_contract() {
        let mut request = WorkerRequest::new(
            ProviderProfile {
                engine: Engine::Claude,
                account: "2".into(),
                path: PathBuf::from("/profiles/claude-2"),
                reserved: false,
            },
            "/tmp/work",
            "Do the work.",
        );
        request.goal = Some("tests pass".into());
        request.max_turns = Some(4);
        let context = WorkerLaunchContext::for_test(request);
        let args = Claude::new("claude")
            .worker_command(&context)
            .unwrap()
            .args_lossy();
        assert_eq!(args[0], "-p");
        assert!(args.contains(&"--dangerously-skip-permissions".into()));
        assert_eq!(
            args[args.iter().position(|item| item == "--model").unwrap() + 1],
            CLAUDE_DEFAULT_MODEL
        );
        assert!(args.last().unwrap().starts_with("/goal tests pass"));
    }
}
