mod credentials;
mod discovery;
mod types;

#[cfg(test)]
#[path = "profiles/profiles_tests.rs"]
mod tests;

use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::Engine;

use super::request::AccountSelector;

use super::prompt::{PromptPort, TERMINAL_PROMPT};
use super::request::AuthMode;
#[cfg(test)]
pub(crate) use discovery::profile_path;
pub(crate) use discovery::{profile_for, profile_path_at};
pub(crate) use types::{DetectedAuth, ManagedProfile};

pub(crate) trait AuthPort: Send + Sync {
    fn profiles(&self, engine: Engine, home: &Path, cwd: &Path) -> Result<Vec<ManagedProfile>>;

    fn ensure_profile(
        &self,
        engine: Engine,
        account: &AccountSelector,
        home: &Path,
        cwd: &Path,
    ) -> Result<ManagedProfile>;

    fn api_key(&self, engine: Engine) -> Result<String> {
        credentials::api_key(engine, self.prompt())
    }

    fn choose_auth_mode(&self, engine: Engine) -> Result<AuthMode> {
        credentials::choose_auth_mode(engine, self.prompt())
    }

    fn set_preferred_auth(
        &self,
        _engine: Engine,
        _profile: &ManagedProfile,
        _mode: AuthMode,
    ) -> Result<()> {
        Ok(())
    }

    fn prompt(&self) -> &dyn PromptPort {
        &TERMINAL_PROMPT
    }

    fn configure_api_key(
        &self,
        _engine: Engine,
        _account: &AccountSelector,
        _home: &Path,
        _cwd: &Path,
        _secret: &str,
    ) -> Result<ManagedProfile> {
        bail!("API-key profile configuration is unavailable")
    }
}

pub(crate) struct FileAuthPort;

impl AuthPort for FileAuthPort {
    fn profiles(&self, engine: Engine, home: &Path, cwd: &Path) -> Result<Vec<ManagedProfile>> {
        discovery::discover(engine, home, cwd)
    }

    fn ensure_profile(
        &self,
        engine: Engine,
        account: &AccountSelector,
        home: &Path,
        cwd: &Path,
    ) -> Result<ManagedProfile> {
        discovery::ensure(engine, account, home, cwd)
    }

    fn configure_api_key(
        &self,
        engine: Engine,
        account: &AccountSelector,
        home: &Path,
        cwd: &Path,
        secret: &str,
    ) -> Result<ManagedProfile> {
        let profile = discovery::ensure(engine, account, home, cwd)?;
        credentials::configure(engine, &profile.profile.path, secret)?;
        Ok(discovery::inspect(
            engine,
            account.label(),
            profile.profile.path,
            home,
        ))
    }

    fn set_preferred_auth(
        &self,
        engine: Engine,
        profile: &ManagedProfile,
        mode: AuthMode,
    ) -> Result<()> {
        credentials::set_preferred_auth(engine, &profile.profile.path, mode)
    }
}
