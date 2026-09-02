use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::providers::{ProviderProfile, WorkerRequest};
use crate::{ConcurrencySettings, EffectiveSettings, Engine, SettingsFile, StatePaths};

pub(crate) fn settings() -> EffectiveSettings {
    EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 8,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        PathBuf::from("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap()
}

pub(crate) fn request(engine: Engine, root: &Path) -> WorkerRequest {
    WorkerRequest::new(
        ProviderProfile {
            engine,
            account: "fixture".into(),
            path: root.join("profiles").join(engine.as_str()),
            reserved: false,
        },
        root,
        "inspect the fixture",
    )
}

pub(crate) fn paths(root: &Path) -> StatePaths {
    StatePaths::new(root, root.join("state"))
}

pub(crate) fn executable(root: &Path, name: &str) -> PathBuf {
    let path = root.join(executable_name(name));
    fs::write(&path, b"fixture executable").unwrap();
    make_executable(&path);
    path
}

fn executable_name(stem: &str) -> String {
    #[cfg(windows)]
    {
        format!("{stem}.exe")
    }
    #[cfg(not(windows))]
    {
        stem.into()
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
