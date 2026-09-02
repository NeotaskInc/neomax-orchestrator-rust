use std::path::{Path, PathBuf};

use crate::atomic::write_bytes_atomic;
use crate::io::{read_file, BoundedIoError, LocalFileSource, ReadLimits};
use crate::runtime::RuntimeEnvironment;
use crate::{Error, Result};

use super::constants::{MAX_SETTINGS_BYTES, SETTINGS_READ_TIMEOUT};
use super::schema::SettingsFile;
use super::validation::validate_concurrency;

impl SettingsFile {
    pub fn discover_path() -> Result<PathBuf> {
        let environment = RuntimeEnvironment::process();
        let home = environment
            .home_dir()
            .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))?;
        let config_root = if environment.platform().is_windows() {
            environment
                .value("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
        } else {
            environment
                .value("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        };
        Ok(config_root.join("neomax").join("config.toml"))
    }

    pub fn path(home: &Path, xdg_config_home: Option<&Path>) -> PathBuf {
        xdg_config_home
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
            .join("neomax")
            .join("config.toml")
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match read_file(
            &LocalFileSource,
            path,
            ReadLimits::new(MAX_SETTINGS_BYTES, SETTINGS_READ_TIMEOUT)
                .expect("settings read limits are valid"),
        ) {
            Ok(bytes) => bytes,
            Err(BoundedIoError::NotFound { .. }) => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        let contents = String::from_utf8(bytes).map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        toml::from_str(&contents).map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        validate_concurrency(&self.concurrency)?;
        let data = toml::to_string_pretty(self)
            .map_err(|error| Error::Message(format!("could not encode settings: {error}")))?;
        write_bytes_atomic(path, data.as_bytes())
    }
}
