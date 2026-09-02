use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::runtime::{self, ResolvedProviderExecutable, RuntimePlatform};
use crate::Result;

pub trait Environment: Send + Sync {
    fn value(&self, key: &str) -> Option<String>;
    fn home_dir(&self) -> Option<PathBuf>;
    fn current_dir(&self) -> PathBuf;

    fn platform(&self) -> RuntimePlatform {
        RuntimePlatform::current()
    }

    fn resolve_path(&self, value: &str) -> PathBuf {
        runtime::resolve_path(
            value,
            self.home_dir().as_deref(),
            &self.current_dir(),
            self.platform(),
        )
    }

    fn temp_dir(&self) -> Option<PathBuf> {
        runtime::temp_dir(self.platform(), |key| self.value(key))
    }

    fn safe_child_environment(&self, provider_config: Option<&str>) -> BTreeMap<String, String> {
        runtime::safe_child_environment(
            self.platform(),
            |key| self.value(key),
            self.home_dir().as_deref(),
            provider_config,
        )
    }

    fn opencode_data_dir(&self, profile: &std::path::Path) -> PathBuf {
        runtime::opencode_data_dir(
            profile,
            &self.home_dir().unwrap_or_default(),
            self.platform(),
            |key| self.value(key),
        )
    }

    fn opencode_config_dir(&self) -> PathBuf {
        runtime::opencode_config_dir(
            &self.home_dir().unwrap_or_default(),
            self.platform(),
            |key| self.value(key),
        )
    }

    fn resolve_provider_executable(&self, program: &str) -> Result<ResolvedProviderExecutable> {
        runtime::resolve_provider_executable(
            program.as_ref(),
            self.platform(),
            self.value("PATH").as_deref().map(std::ffi::OsStr::new),
            self.value("PATHEXT").as_deref().map(std::ffi::OsStr::new),
            self.value("ComSpec").as_deref().map(std::ffi::OsStr::new),
            self.value("SystemRoot")
                .as_deref()
                .map(std::ffi::OsStr::new),
            &self.current_dir(),
        )
    }
}

pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn value(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        let environment = runtime::RuntimeEnvironment::process();
        environment.home_dir()
    }

    fn current_dir(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_default()
    }
}

#[derive(Clone, Default)]
pub struct MapEnvironment {
    values: BTreeMap<String, String>,
    home: Option<PathBuf>,
    current_dir: Option<PathBuf>,
    platform: RuntimePlatform,
}

impl MapEnvironment {
    pub fn new(values: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            values: values.into_iter().collect(),
            home: None,
            current_dir: None,
            platform: RuntimePlatform::current(),
        }
    }

    pub fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.home = Some(home.into());
        self
    }

    pub fn with_current_dir(mut self, current_dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(current_dir.into());
        self
    }

    pub fn with_platform(mut self, platform: RuntimePlatform) -> Self {
        self.platform = platform;
        self
    }
}

impl Environment for MapEnvironment {
    fn value(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home
            .clone()
            .or_else(|| runtime::native_home(self.platform, |key| self.values.get(key).cloned()))
    }

    fn current_dir(&self) -> PathBuf {
        self.current_dir.clone().unwrap_or_default()
    }

    fn platform(&self) -> RuntimePlatform {
        self.platform
    }
}
