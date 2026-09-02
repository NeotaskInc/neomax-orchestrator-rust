use std::ffi::{OsStr, OsString};

use crate::providers::catalog::{CODEX_DEFAULT_MODEL, CODEX_SERVICE_TIER};
use crate::providers::worker::{apply_profile, base_command, composed_prompt};
use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext, auth,
    catalog, events,
};
use crate::{Engine, Result};

pub struct Codex {
    binary: OsString,
}

impl Codex {
    pub fn new(binary: impl Into<OsString>) -> Self {
        Self {
            binary: binary.into(),
        }
    }
}

impl Provider for Codex {
    fn engine(&self) -> Engine {
        Engine::Codex
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        CODEX_DEFAULT_MODEL
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
            .arg("exec")
            .arg("--json");
        command = if request.plan {
            command.arg("-s").arg("read-only")
        } else {
            command.arg("--dangerously-bypass-approvals-and-sandbox")
        };
        command = command
            .arg("-m")
            .arg(request.selected_model())
            .arg("-c")
            .arg(format!(
                "model_reasoning_effort={}",
                request
                    .effort
                    .as_deref()
                    .unwrap_or(if request.ultra { "xhigh" } else { "high" })
            ))
            .arg("-c")
            .arg(format!("service_tier={CODEX_SERVICE_TIER}"))
            .arg("-C")
            .arg(request.cwd.as_os_str())
            .arg("--skip-git-repo-check")
            .arg(composed_prompt(context, None))
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0");
        apply_profile(command, request, None)
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(events::parse_codex(bytes))
    }

    fn refresh_quota(
        &self,
        profile: &std::path::Path,
        session_id: Option<&str>,
        observed_at: f64,
    ) -> Result<Option<crate::providers::CodexQuotaRefreshResult>> {
        events::refresh_from_rollout(profile, session_id, observed_at)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::providers::WorkerRequest;

    use super::*;

    #[test]
    fn pins_fast_mode_effort_model_and_workdir() {
        let profile = ProviderProfile {
            engine: Engine::Codex,
            account: "2".into(),
            path: PathBuf::from("/profiles/codex-2"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.model = Some("gpt-5.5".into());
        request.effort = Some("xhigh".into());
        request.goal = Some("tests pass".into());
        let context = WorkerLaunchContext::for_test(request);
        let args = Codex::new("codex")
            .worker_command(&context)
            .unwrap()
            .args_lossy();
        assert_eq!(&args[..2], ["exec", "--json"]);
        assert!(args.contains(&"model_reasoning_effort=xhigh".into()));
        assert!(args.contains(&"service_tier=fast".into()));
        assert!(args.last().unwrap().contains("OBJECTIVE"));
    }

    #[test]
    fn ultra_maps_to_codex_xhigh_reasoning() {
        let profile = ProviderProfile {
            engine: Engine::Codex,
            account: "1".into(),
            path: PathBuf::from("/profiles/codex-1"),
            reserved: false,
        };
        let mut request = WorkerRequest::new(profile, "/tmp/work", "Do the work.");
        request.ultra = true;
        let context = WorkerLaunchContext::for_test(request);
        let args = Codex::new("codex")
            .worker_command(&context)
            .unwrap()
            .args_lossy();
        assert!(args.contains(&"model_reasoning_effort=xhigh".into()));
    }
}
