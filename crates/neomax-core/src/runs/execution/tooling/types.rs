use std::ffi::OsString;
use std::path::PathBuf;

use crate::agent_tools::ExecutableInputs;
use crate::installation::InstallPaths;
use crate::providers::WorkerRequest;
use crate::{EffectiveSettings, StatePaths};

#[derive(Debug, Clone)]
pub struct WorkerToolingInput<'a> {
    pub paths: &'a StatePaths,
    pub settings: &'a EffectiveSettings,
    pub request: &'a WorkerRequest,
    pub executable_inputs: ExecutableInputs,
    pub ambient_path: Option<OsString>,
    pub inherited_depth: Option<String>,
    pub inherited_max_depth: Option<String>,
}

impl<'a> WorkerToolingInput<'a> {
    pub fn from_runtime(
        paths: &'a StatePaths,
        settings: &'a EffectiveSettings,
        request: &'a WorkerRequest,
    ) -> Self {
        Self {
            paths,
            settings,
            request,
            executable_inputs: ExecutableInputs::new(
                std::env::current_exe().ok(),
                InstallPaths::discover()
                    .ok()
                    .map(|install| install.neomax_binary())
                    .or_else(|| Some(default_install_bin(paths))),
            ),
            ambient_path: std::env::var_os("PATH"),
            inherited_depth: std::env::var(crate::agent_tools::NEOMAX_TOOL_DEPTH_ENV).ok(),
            inherited_max_depth: std::env::var(crate::agent_tools::NEOMAX_TOOL_MAX_DEPTH_ENV).ok(),
        }
    }
}

pub(crate) fn manifest_path(paths: &StatePaths) -> PathBuf {
    paths.state.join(crate::agent_tools::MANIFEST_RELATIVE_PATH)
}

pub(crate) fn default_install_bin(paths: &StatePaths) -> PathBuf {
    #[cfg(windows)]
    {
        paths.home.join(".local").join("bin").join("neomax.exe")
    }
    #[cfg(not(windows))]
    {
        paths.home.join(".local").join("bin").join("neomax")
    }
}
