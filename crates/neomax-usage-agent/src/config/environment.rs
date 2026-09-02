use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;
use neomax_core::io::path_to_string;
use neomax_core::providers::catalog::all_specs;

use super::binary::{resolve_binary, resolve_required_binary};
use super::paths::AgentPaths;
use super::validation::{
    absolute_env_path, require_absolute, validated_path, validated_provider_value,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceEnvironment {
    values: BTreeMap<String, String>,
}

impl ServiceEnvironment {
    pub(super) fn for_paths(paths: &AgentPaths, executable: &Path, neomax_cli: &Path) -> Self {
        let mut values = BTreeMap::new();
        values.insert(
            "NEOMAX_HOME".into(),
            paths.state.state.display().to_string(),
        );
        values.insert(
            "NEOMAX_USAGE_AGENT_BIN".into(),
            executable.display().to_string(),
        );
        values.insert("NEOMAX_CLI_BIN".into(), neomax_cli.display().to_string());
        values.insert("HOME".into(), paths.home.display().to_string());
        let config_home = paths
            .systemd_unit
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home.join(".config"));
        values.insert("XDG_CONFIG_HOME".into(), config_home.display().to_string());
        values.insert(
            "XDG_DATA_HOME".into(),
            paths
                .home
                .join(".local")
                .join("share")
                .display()
                .to_string(),
        );
        values.insert("USERPROFILE".into(), paths.home.display().to_string());
        let appdata = paths
            .windows_task_xml
            .parent()
            .and_then(Path::parent)
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home.join("AppData").join("Roaming"));
        values.insert("APPDATA".into(), appdata.display().to_string());
        values.insert(
            "LOCALAPPDATA".into(),
            paths
                .home
                .join("AppData")
                .join("Local")
                .display()
                .to_string(),
        );
        if let Some(path) = env::var_os("PATH") {
            values.insert("PATH".into(), path.to_string_lossy().into_owned());
        }
        Self { values }
    }

    pub(super) fn discover(
        paths: &AgentPaths,
        executable: &Path,
        neomax_cli: &Path,
    ) -> Result<Self> {
        paths.validate()?;
        require_absolute("HOME", &paths.home)?;
        require_absolute("NEOMAX_HOME", &paths.state.state)?;
        require_absolute("NEOMAX_USAGE_AGENT_BIN", executable)?;
        require_absolute("NEOMAX_CLI_BIN", neomax_cli)?;
        let path_value = env::var_os("PATH");
        let path = validated_path(path_value.as_deref(), &[executable, neomax_cli])?;
        let mut values = BTreeMap::new();
        values.insert(
            "NEOMAX_HOME".into(),
            path_to_string("NEOMAX_HOME", &paths.state.state)?,
        );
        values.insert(
            "NEOMAX_USAGE_AGENT_BIN".into(),
            path_to_string("NEOMAX_USAGE_AGENT_BIN", executable)?,
        );
        values.insert(
            "NEOMAX_CLI_BIN".into(),
            path_to_string("NEOMAX_CLI_BIN", neomax_cli)?,
        );

        let home = paths.home.clone();
        let home_text = path_to_string("HOME", &home)?;
        values.insert("HOME".into(), home_text.clone());
        values.insert("USERPROFILE".into(), home_text);
        values.insert(
            "XDG_CONFIG_HOME".into(),
            absolute_env_path("XDG_CONFIG_HOME", home.join(".config"))?,
        );
        values.insert(
            "XDG_DATA_HOME".into(),
            absolute_env_path("XDG_DATA_HOME", home.join(".local").join("share"))?,
        );
        values.insert(
            "APPDATA".into(),
            absolute_env_path("APPDATA", home.join("AppData").join("Roaming"))?,
        );
        values.insert(
            "LOCALAPPDATA".into(),
            absolute_env_path("LOCALAPPDATA", home.join("AppData").join("Local"))?,
        );
        values.insert("PATH".into(), path);

        for provider in all_specs() {
            let binary = env::var_os(&provider.binary_env)
                .map(|raw| {
                    resolve_required_binary(&provider.binary_env, &raw, path_value.as_deref())
                })
                .transpose()?;
            let binary = binary
                .or_else(|| resolve_binary(&provider.default_binary, path_value.as_deref()).ok());
            if let Some(binary) = binary {
                values.insert(
                    provider.binary_env.clone(),
                    path_to_string(&provider.binary_env, &binary)?,
                );
            }
            for key in [
                &provider.config_env,
                &provider.profile_env,
                &provider.orchestrator_env,
            ] {
                if let Some(raw) = env::var_os(key) {
                    values.insert((*key).clone(), validated_provider_value(key, &raw)?);
                }
            }
        }
        Ok(Self { values })
    }

    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.values
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    }

    #[cfg(test)]
    pub(crate) fn with_value(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.into(), value.into());
        self
    }
}
