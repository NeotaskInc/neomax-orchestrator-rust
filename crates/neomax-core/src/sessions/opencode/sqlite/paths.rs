use std::path::{Path, PathBuf};

use crate::runtime::{RuntimeEnvironment, RuntimePlatform, opencode_data_dir};

pub fn database_path(profile: &Path, home: &Path) -> PathBuf {
    data_dir(profile, home).join("opencode.db")
}

pub fn data_dir(profile: &Path, home: &Path) -> PathBuf {
    let default_profile = home.join(".opencode");
    let is_default = profile == default_profile
        || profile
            .canonicalize()
            .ok()
            .zip(default_profile.canonicalize().ok())
            .is_some_and(|(profile, default)| profile == default);
    if is_default {
        home.join(".local/share/opencode")
    } else {
        profile.join("opencode")
    }
}

pub fn data_dir_for_environment(
    profile: &Path,
    home: &Path,
    environment: &RuntimeEnvironment,
) -> PathBuf {
    if environment.home_dir().as_deref() == Some(home) {
        return environment.opencode_data_dir(profile);
    }
    opencode_data_dir(profile, home, RuntimePlatform::current(), |_| None)
}

pub fn database_path_for_environment(
    profile: &Path,
    home: &Path,
    environment: &RuntimeEnvironment,
) -> PathBuf {
    data_dir_for_environment(profile, home, environment).join("opencode.db")
}
