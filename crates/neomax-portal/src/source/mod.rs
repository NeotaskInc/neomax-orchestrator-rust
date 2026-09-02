pub(crate) mod accounts;
pub(crate) mod events;
mod files;
mod history;
pub(crate) mod issues;
mod local_usage;
pub(crate) mod modes;
pub(crate) mod plans;
pub(crate) mod runs;
mod sessions;
pub(crate) mod state;
mod usage;
pub(crate) mod worktrees;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, bail};

use neomax_core::config::StatePaths;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::providers::ProviderProfile;
use neomax_core::providers::catalog::{
    CatalogSnapshot, MapEnvironment, ProviderSnapshot, RealFileSystem,
};
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::HistorySummary;
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::UsageReport;

use crate::args::PortalArgs;
use crate::model::{ModesResponse, PortalSnapshot, RunDiff};

pub(crate) const SESSION_ACTIVITY_WINDOW_SECONDS: i64 = 240;

pub use files::read_run_log;

pub trait PortalSource: Send + Sync {
    fn status(&self, now: i64, days: u32) -> Result<PortalSnapshot>;
    fn history(&self, limit: usize) -> Result<Vec<HistorySummary>>;
    fn modes(&self) -> Result<ModesResponse>;
    fn usage(&self, days: u32, now: i64) -> Result<UsageReport>;
    fn sessions(&self, days: u32, now: i64) -> Result<Vec<SessionRecord>>;
    fn run_diff(&self, id: &str) -> Result<RunDiff>;
    fn run_log(&self, id: &str, limit: usize) -> Result<String>;

    fn action_context(&self) -> crate::actions::ActionContext {
        crate::actions::ActionContext::from_environment()
            .unwrap_or_else(|_| crate::actions::ActionContext::from_home(".", ".neomax"))
    }
}

#[derive(Clone)]
pub struct FilesystemPortalSource {
    pub(crate) home: PathBuf,
    pub(crate) paths: StatePaths,
    pub(crate) max_artifact_bytes: usize,
    pub(crate) discovery_environment: MapEnvironment,
    provider_catalog: Option<Arc<CatalogSnapshot>>,
}

impl fmt::Debug for FilesystemPortalSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FilesystemPortalSource")
            .field("home", &self.home)
            .field("paths", &self.paths)
            .field("max_artifact_bytes", &self.max_artifact_bytes)
            .field("provider_catalog", &self.provider_catalog)
            .finish()
    }
}

impl FilesystemPortalSource {
    pub fn new(home: impl Into<PathBuf>, state: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let paths = StatePaths::new(home.clone(), state);
        Self {
            discovery_environment: MapEnvironment::new(std::iter::empty()).with_home(home.clone()),
            home,
            paths,
            max_artifact_bytes: 128 * 1024 * 1024,
            provider_catalog: None,
        }
    }

    pub fn with_max_artifact_bytes(mut self, max_bytes: usize) -> Self {
        self.max_artifact_bytes = max_bytes.max(1);
        self
    }

    pub fn with_discovery_environment(mut self, environment: MapEnvironment) -> Self {
        self.discovery_environment = environment;
        self
    }

    pub fn with_catalog(mut self, catalog: CatalogSnapshot) -> Self {
        self.provider_catalog = Some(Arc::new(catalog));
        self
    }

    pub fn with_catalog_arc(mut self, catalog: Arc<CatalogSnapshot>) -> Self {
        self.provider_catalog = Some(catalog);
        self
    }

    pub fn with_provider_runtime(self, runtime: &ProviderRuntime) -> Self {
        self.with_catalog_arc(runtime.catalog_arc())
    }

    pub fn from_args(args: &PortalArgs) -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let home = args
            .home
            .clone()
            .or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
            })
            .ok_or_else(|| anyhow::anyhow!("HOME is not set; use --home PATH"))?;
        let home = absolute_root(home, &current_dir, "portal home")?;
        let state = args
            .state
            .clone()
            .or_else(|| std::env::var_os("NEOMAX_HOME").map(PathBuf::from))
            .unwrap_or_else(|| home.join(".neomax"));
        let state = absolute_root(state, &current_dir, "Neomax state")?;
        let environment = MapEnvironment::new(std::env::vars())
            .with_home(home.clone())
            .with_current_dir(current_dir);
        let runtime = ProviderRuntime::discover_process()?;
        Ok(Self::new(home, state)
            .with_discovery_environment(environment)
            .with_provider_runtime(&runtime)
            .with_max_artifact_bytes(args.max_artifact_bytes))
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn paths(&self) -> &StatePaths {
        &self.paths
    }

    pub fn provider_catalog(&self) -> Option<&CatalogSnapshot> {
        self.provider_catalog.as_deref()
    }

    pub fn provider_snapshot(&self, engine: neomax_core::Engine) -> Option<&ProviderSnapshot> {
        self.provider_catalog()?.providers.get(&engine)
    }

    pub fn provider_profiles(&self, engine: neomax_core::Engine) -> Result<Vec<ProviderProfile>> {
        if let Some(snapshot) = self.provider_snapshot(engine) {
            return Ok(neomax_core::providers::catalog::provider_profiles(snapshot));
        }
        Ok(neomax_core::providers::catalog::discover_profile_snapshots(
            engine,
            &self.discovery_environment,
            &RealFileSystem,
        )?
        .into_iter()
        .map(|profile| ProviderProfile {
            engine: profile.engine,
            account: profile.account,
            path: profile.path,
            reserved: profile.reserved,
        })
        .collect())
    }
}

fn absolute_root(path: PathBuf, current_dir: &Path, label: &str) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(&path) {
        bail!("{label} must not be rooted without an absolute prefix")
    }
    if path.is_absolute() {
        return Ok(path);
    }
    if !current_dir.is_absolute() || is_rooted_but_not_absolute(current_dir) {
        bail!("current directory must be absolute")
    }
    Ok(current_dir.join(path))
}

impl PortalSource for FilesystemPortalSource {
    fn status(&self, now: i64, days: u32) -> Result<PortalSnapshot> {
        crate::aggregate::build_status(self, now, days)
    }

    fn history(&self, limit: usize) -> Result<Vec<HistorySummary>> {
        history::read_history(self, limit)
    }

    fn modes(&self) -> Result<ModesResponse> {
        Ok(modes::available_modes(self))
    }

    fn usage(&self, days: u32, now: i64) -> Result<UsageReport> {
        usage::read_usage(self, days, now)
    }

    fn sessions(&self, days: u32, now: i64) -> Result<Vec<SessionRecord>> {
        sessions::discover_sessions(self, days, now)
    }

    fn run_diff(&self, id: &str) -> Result<RunDiff> {
        files::run_diff(self, id)
    }

    fn run_log(&self, id: &str, limit: usize) -> Result<String> {
        files::run_log(self, id, limit)
    }

    fn action_context(&self) -> crate::actions::ActionContext {
        crate::actions::ActionContext::from_home(self.home.clone(), self.paths.state.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_keeps_state_and_home_separate_for_relocated_installations() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let state = temp.path().join("state");
        let source = FilesystemPortalSource::new(&home, &state);
        assert_eq!(source.home(), home);
        assert_eq!(source.paths().state, state);
    }

    #[test]
    fn relative_roots_resolve_beneath_an_absolute_current_directory() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            absolute_root(PathBuf::from("state"), temp.path(), "state").unwrap(),
            temp.path().join("state")
        );
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_roots_are_rejected() {
        let current_dir = Path::new(r"C:\workspace");
        for path in [PathBuf::from(r"\state"), PathBuf::from(r"C:state")] {
            assert!(absolute_root(path, current_dir, "state").is_err());
        }
    }
}
