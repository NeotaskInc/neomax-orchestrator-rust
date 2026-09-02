mod cli;
mod cli_surface;
mod config;
mod dispatch_completeness;
mod installation;
mod parser;
mod projects;
mod queue;
mod tasks;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use neomax_core::orchestration::registry::OrchestratorLiveness;
use neomax_core::{EffectiveSettings, SettingsFile, StatePaths};
use tempfile::TempDir;

use crate::context::RuntimeContext;

pub struct Fixture {
    pub _temp: TempDir,
    pub context: RuntimeContext,
}

pub fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("temporary fixture directory");
    let home = temp.path().join("home");
    let cwd = temp.path().join("workspace");
    let state = temp.path().join("state");
    fs::create_dir_all(&home).expect("fixture home");
    fs::create_dir_all(&cwd).expect("fixture cwd");
    let paths = StatePaths::new(home, state);
    paths.ensure_runtime_dirs().expect("fixture state");
    let settings = EffectiveSettings::resolve(
        SettingsFile::default(),
        paths.state.join("config.toml"),
        &BTreeMap::new(),
    )
    .expect("fixture settings");
    let context = RuntimeContext::for_test(
        paths,
        settings,
        cwd,
        1_700_000_000,
        OrchestratorLiveness::default(),
        None,
    );
    Fixture {
        _temp: temp,
        context,
    }
}

pub fn seed_path(fixture: &Fixture) -> PathBuf {
    fixture.context.paths.state.join("projects.local.json")
}
