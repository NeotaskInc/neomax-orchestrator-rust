//! Injectable process environment and provider runtime roots.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
#[cfg(any(test, windows))]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::runtime::executable::{ResolvedProviderExecutable, resolve_provider_executable};
use crate::runtime::paths::{
    native_home, opencode_config_dir, opencode_data_dir, resolve_path, safe_child_environment,
    temp_dir,
};
use crate::runtime::platform::RuntimePlatform;

/// A deterministic, injectable view of process runtime inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEnvironment {
    platform: RuntimePlatform,
    values: BTreeMap<String, String>,
    current_dir: PathBuf,
}

impl RuntimeEnvironment {
    pub fn process() -> Self {
        Self {
            platform: RuntimePlatform::current(),
            values: env::vars().collect(),
            current_dir: env::current_dir().unwrap_or_default(),
        }
    }

    pub fn fixture(
        platform: RuntimePlatform,
        values: impl IntoIterator<Item = (String, String)>,
        current_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            platform,
            values: values.into_iter().collect(),
            current_dir: current_dir.into(),
        }
    }

    pub fn platform(&self) -> RuntimePlatform {
        self.platform
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn current_dir(&self) -> &Path {
        &self.current_dir
    }

    pub fn home_dir(&self) -> Option<PathBuf> {
        native_home(self.platform, |key| self.value(key).map(str::to_owned))
    }

    pub fn temp_dir(&self) -> Option<PathBuf> {
        temp_dir(self.platform, |key| self.value(key).map(str::to_owned))
    }

    pub fn resolve_path(&self, value: &str) -> PathBuf {
        resolve_path(
            value,
            self.home_dir().as_deref(),
            &self.current_dir,
            self.platform,
        )
    }

    pub fn safe_child_environment(
        &self,
        provider_config: Option<&str>,
    ) -> BTreeMap<String, String> {
        safe_child_environment(
            self.platform,
            |key| self.value(key).map(str::to_owned),
            self.home_dir().as_deref(),
            provider_config,
        )
    }

    pub fn opencode_data_dir(&self, profile: &Path) -> PathBuf {
        opencode_data_dir(
            profile,
            &self.home_dir().unwrap_or_default(),
            self.platform,
            |key| self.value(key).map(str::to_owned),
        )
    }

    pub fn opencode_config_dir(&self) -> PathBuf {
        opencode_config_dir(&self.home_dir().unwrap_or_default(), self.platform, |key| {
            self.value(key).map(str::to_owned)
        })
    }

    pub fn opencode_auth_path(&self, profile: &Path) -> PathBuf {
        self.opencode_data_dir(profile).join("auth.json")
    }

    pub fn resolve_provider_executable(
        &self,
        program: impl AsRef<OsStr>,
    ) -> Result<ResolvedProviderExecutable> {
        self.resolve_provider_executable_at(program, &self.current_dir)
    }

    pub fn resolve_provider_executable_at(
        &self,
        program: impl AsRef<OsStr>,
        current_dir: &Path,
    ) -> Result<ResolvedProviderExecutable> {
        resolve_provider_executable(
            program.as_ref(),
            self.platform,
            self.value("PATH").map(OsStr::new),
            self.value("PATHEXT").map(OsStr::new),
            self.value("ComSpec")
                .or_else(|| self.value("COMSPEC"))
                .map(OsStr::new),
            self.value("SystemRoot")
                .or_else(|| self.value("SYSTEMROOT"))
                .map(OsStr::new),
            current_dir,
        )
    }

    #[cfg(windows)]
    pub(crate) fn command_shell(&self) -> Result<OsString> {
        super::executable::resolve_command_shell(
            self.value("ComSpec")
                .or_else(|| self.value("COMSPEC"))
                .map(OsStr::new),
            self.value("SystemRoot")
                .or_else(|| self.value("SYSTEMROOT"))
                .map(OsStr::new),
        )
    }

    #[cfg(test)]
    pub(crate) fn resolve_provider_command_at<I, S>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        current_dir: &Path,
    ) -> Result<(OsString, Vec<OsString>)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        self.resolve_provider_executable_at(program, current_dir)
            .and_then(|resolved| resolved.apply_to_command(&args))
    }

    pub fn process_command<I, S>(
        &self,
        program: impl AsRef<OsStr>,
        args: I,
        current_dir: &Path,
    ) -> Result<Command>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        let resolved = self.resolve_provider_executable_at(program, current_dir)?;
        resolved.process_command(&args)
    }
}

pub fn process_command<I, S>(
    program: impl AsRef<OsStr>,
    args: I,
    current_dir: &Path,
) -> Result<Command>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    RuntimeEnvironment::process().process_command(program, args, current_dir)
}
