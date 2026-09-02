use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use neomax_core::config::{Engine, StatePaths};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::providers::catalog::CatalogSnapshot;
use neomax_core::providers::runtime::ProviderRuntime;

use super::SERVICE_LABEL;

#[derive(Debug, Clone)]
pub struct AgentPaths {
    pub home: PathBuf,
    pub state: StatePaths,
    pub launchd_plist: PathBuf,
    pub systemd_unit: PathBuf,
    pub windows_task_xml: PathBuf,
    provider_catalog: Option<Arc<CatalogSnapshot>>,
}

impl AgentPaths {
    pub fn discover() -> Result<Self> {
        let state = StatePaths::discover()?;
        let paths = Self::for_discovered_state(state);
        paths.validate()?;
        let runtime = ProviderRuntime::discover_process()?;
        Ok(paths.with_catalog_arc(runtime.catalog_arc()))
    }

    pub fn for_state(state: StatePaths) -> Self {
        Self::for_state_with_roots(
            state.clone(),
            state.home.join(".config"),
            state.home.join("AppData").join("Roaming"),
        )
    }

    pub(super) fn for_discovered_state(state: StatePaths) -> Self {
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| state.home.join(".config"));
        let appdata = env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| state.home.join("AppData").join("Roaming"));
        Self::for_state_with_roots(state, config_home, appdata)
    }

    pub fn for_state_with_roots(state: StatePaths, config_home: PathBuf, appdata: PathBuf) -> Self {
        let launchd_plist = state
            .home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{SERVICE_LABEL}.plist"));
        let systemd_unit = config_home
            .join("systemd")
            .join("user")
            .join("neomax-usage-agent.service");
        let windows_task_xml = appdata.join("Neomax").join("neomax-usage-agent.xml");
        Self {
            home: state.home.clone(),
            state,
            launchd_plist,
            systemd_unit,
            windows_task_xml,
            provider_catalog: None,
        }
    }

    pub fn with_catalog(mut self, catalog: CatalogSnapshot) -> Self {
        self.provider_catalog = Some(Arc::new(catalog));
        self
    }

    pub fn with_catalog_arc(mut self, catalog: Arc<CatalogSnapshot>) -> Self {
        self.provider_catalog = Some(catalog);
        self
    }

    pub fn provider_catalog(&self) -> Option<&CatalogSnapshot> {
        self.provider_catalog.as_deref()
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for (label, path) in [
            ("usage-agent home path", self.home.as_path()),
            ("usage-agent state path", self.state.state.as_path()),
            ("launchd service path", self.launchd_plist.as_path()),
            ("systemd service path", self.systemd_unit.as_path()),
            ("Windows service path", self.windows_task_xml.as_path()),
        ] {
            if !path.is_absolute() || is_rooted_but_not_absolute(path) {
                bail!("{label} must be an absolute path")
            }
        }
        Ok(())
    }

    pub fn profile_root(&self, engine: Engine) -> PathBuf {
        self.home
            .join(&neomax_core::providers::catalog::spec(engine).default_profile_dir)
    }
}
