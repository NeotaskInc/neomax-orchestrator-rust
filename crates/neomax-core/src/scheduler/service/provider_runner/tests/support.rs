use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::path::PathBuf;

use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, WorkerLaunchContext,
};
use crate::Engine;

pub(super) struct FixtureProvider {
    pub(super) profile: ProviderProfile,
}

impl Provider for FixtureProvider {
    fn engine(&self) -> Engine {
        Engine::Claude
    }

    fn binary(&self) -> &OsStr {
        OsStr::new("fixture-provider")
    }

    fn default_model(&self) -> &str {
        "fixture-model"
    }

    fn profiles(&self) -> crate::Result<Vec<ProviderProfile>> {
        Ok(vec![self.profile.clone()])
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> crate::Result<ProviderCommand> {
        Ok(ProviderCommand::new(
            OsString::from("fixture-provider"),
            context.request().cwd.clone(),
        ))
    }

    fn parse_events(&self, _bytes: &[u8]) -> crate::Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }
}

#[cfg(unix)]
pub(super) struct ScriptProvider {
    pub(super) engine: Engine,
    pub(super) profile: ProviderProfile,
    pub(super) executable: PathBuf,
    pub(super) default_model: String,
    pub(super) model_capture: Option<PathBuf>,
}

#[cfg(unix)]
impl Provider for ScriptProvider {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn binary(&self) -> &OsStr {
        self.executable.as_os_str()
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn profiles(&self) -> crate::Result<Vec<ProviderProfile>> {
        Ok(vec![self.profile.clone()])
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(&self, context: &WorkerLaunchContext) -> crate::Result<ProviderCommand> {
        let mut command = ProviderCommand::new(&self.executable, context.request().cwd.clone())
            .arg(context.request().selected_model());
        if let Some(path) = &self.model_capture {
            command = command.env("MODEL_CAPTURE", path);
        }
        Ok(command)
    }

    fn parse_events(&self, _bytes: &[u8]) -> crate::Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }
}
