use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::registry::{OrchestratorLiveness, OrchestratorStore};
use neomax_core::projects::ProjectRegistry;
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::SystemProcessProbe;
use neomax_core::{EffectiveSettings, StatePaths};

use crate::models::{self, ModelOverrides};

pub struct RuntimeContext {
    pub paths: StatePaths,
    pub settings: EffectiveSettings,
    pub cwd: PathBuf,
    pub now: i64,
    pub liveness: OrchestratorLiveness,
    provider_runtime: Option<ProviderRuntime>,
    local_seed: Option<PathBuf>,
}

impl RuntimeContext {
    pub fn discover() -> Result<Self> {
        let paths = StatePaths::discover()?;
        let settings = EffectiveSettings::discover()?;
        let cwd = env::current_dir()?;
        let now = unix_now();
        let provider_runtime = ProviderRuntime::discover_process()?;
        paths.ensure_runtime_dirs()?;
        let store = OrchestratorStore::new(&paths.orchestrators);
        let liveness = OrchestratorLiveness::load(&store, &SystemProcessProbe, now)?;
        let explicit_seed = env::var_os("NEOMAX_PROJECTS_CONFIG");
        let alternate_state = env::var_os("NEOMAX_HOME").is_some();
        let local_seed = discover_local_seed(&cwd, explicit_seed.as_deref(), alternate_state);
        let registry = ProjectRegistry::new(&paths.projects, local_seed.clone());
        registry.ensure_launch_project(&cwd, &paths.home, None, now)?;
        Ok(Self {
            paths,
            settings,
            cwd,
            now,
            liveness,
            provider_runtime: Some(provider_runtime),
            local_seed,
        })
    }

    #[cfg(test)]
    pub fn for_test(
        paths: StatePaths,
        settings: EffectiveSettings,
        cwd: PathBuf,
        now: i64,
        liveness: OrchestratorLiveness,
        local_seed: Option<PathBuf>,
    ) -> Self {
        Self {
            paths,
            settings,
            cwd,
            now,
            liveness,
            provider_runtime: Some(ProviderRuntime::empty()),
            local_seed,
        }
    }

    pub fn project_registry(&self) -> ProjectRegistry {
        ProjectRegistry::new(&self.paths.projects, self.local_seed.clone())
    }

    pub fn project_for_cwd(&self) -> Option<String> {
        self.project_registry().project_of(&self.cwd)
    }

    pub fn resolve_path(&self, value: &str) -> PathBuf {
        resolve_path_from_cwd(&self.cwd, value)
    }

    pub fn model_config_path(&self) -> PathBuf {
        models::config_path(&self.settings.config_path)
    }

    pub fn model_overrides(&self) -> Result<ModelOverrides> {
        Ok(ModelOverrides::load(&self.model_config_path())?)
    }

    pub fn provider_runtime(&self) -> Result<ProviderRuntime> {
        Ok(self
            .provider_runtime
            .clone()
            .map(Ok)
            .unwrap_or_else(ProviderRuntime::discover_process)?)
    }
}

fn resolve_path_from_cwd(cwd: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if is_rooted_but_not_absolute(path) {
        return cwd.to_path_buf();
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn discover_local_seed(
    cwd: &Path,
    explicit_config: Option<&std::ffi::OsStr>,
    alternate_state: bool,
) -> Option<PathBuf> {
    if let Some(path) = explicit_config {
        let path = PathBuf::from(path);
        if is_rooted_but_not_absolute(&path) {
            return None;
        }
        return Some(if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        });
    }

    // The repository-local seed is a convenience for the canonical install.
    // Alternate state homes must stay isolated, especially in tests.
    if alternate_state {
        return None;
    }
    let candidate = cwd.join("project/projects.local.json");
    candidate.is_file().then_some(candidate)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn local_seed_autodiscovery_is_limited_to_the_canonical_state_home() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("workspace");
        let seed = cwd.join("project/projects.local.json");
        fs::create_dir_all(seed.parent().unwrap()).unwrap();
        fs::write(&seed, b"{}").unwrap();

        assert_eq!(
            discover_local_seed(&cwd, None, false).as_deref(),
            Some(seed.as_path())
        );
        assert_eq!(discover_local_seed(&cwd, None, true), None);
    }

    #[test]
    fn explicit_seed_path_is_honored_for_an_alternate_state_home() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).unwrap();
        let configured = temp.path().join("configured-seed.json");
        assert_eq!(
            discover_local_seed(&cwd, Some(configured.as_os_str()), true).as_deref(),
            Some(configured.as_path())
        );

        let relative = std::ffi::OsStr::new("seed.json");
        assert_eq!(
            discover_local_seed(&cwd, Some(relative), true).as_deref(),
            Some(cwd.join("seed.json").as_path())
        );
    }

    #[cfg(windows)]
    #[test]
    fn partial_roots_do_not_escape_the_launch_directory() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("workspace");
        fs::create_dir_all(&cwd).unwrap();

        for partial_root in [r"\seed.json", r"C:seed.json"] {
            assert_eq!(
                discover_local_seed(&cwd, Some(std::ffi::OsStr::new(partial_root)), true),
                None
            );
            assert_eq!(resolve_path_from_cwd(&cwd, partial_root), cwd);
        }
    }
}
