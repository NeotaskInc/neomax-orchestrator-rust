use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorIdentity {
    pub engine: Engine,
    pub profile: PathBuf,
}

pub fn infer_engine(environment: &BTreeMap<String, String>) -> Engine {
    if let Some(role) = environment.get("NEOMAX_ROLE") {
        if let Ok(engine) = role.parse() {
            return engine;
        }
    }
    if present(environment, "GROK_HOME") && !present(environment, "CLAUDE_CONFIG_DIR") {
        return Engine::Grok;
    }
    if present(environment, "KIMI_CODE_HOME") && !present(environment, "CLAUDE_CONFIG_DIR") {
        return Engine::Kimi;
    }
    if present(environment, "CODEX_HOME") && !present(environment, "CLAUDE_CONFIG_DIR") {
        return Engine::Codex;
    }
    Engine::Claude
}

pub fn identity(
    environment: &BTreeMap<String, String>,
    home: &Path,
    cwd: &Path,
) -> OrchestratorIdentity {
    let engine = infer_engine(environment);
    OrchestratorIdentity {
        engine,
        profile: current_profile(engine, environment, home, cwd),
    }
}

pub fn current_profile(
    engine: Engine,
    environment: &BTreeMap<String, String>,
    home: &Path,
    cwd: &Path,
) -> PathBuf {
    let raw = environment
        .get(config_env(engine))
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| profile_for_engine(engine, home));
    let fallback = profile_for_engine(engine, home);
    absolute_path(raw, cwd)
        .or_else(|| absolute_path(fallback.clone(), cwd))
        .unwrap_or(fallback)
}

pub fn profile_for_engine(engine: Engine, home: &Path) -> PathBuf {
    home.join(match engine {
        Engine::Claude => ".claude",
        Engine::Codex => ".codex",
        Engine::Opencode => ".opencode",
        Engine::Kimi => ".kimi-code",
        Engine::Grok => ".grok",
    })
}

pub const fn config_env(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "CLAUDE_CONFIG_DIR",
        Engine::Codex => "CODEX_HOME",
        Engine::Opencode => "XDG_DATA_HOME",
        Engine::Kimi => "KIMI_CODE_HOME",
        Engine::Grok => "GROK_HOME",
    }
}

fn present(environment: &BTreeMap<String, String>, key: &str) -> bool {
    environment
        .get(key)
        .is_some_and(|value| !value.trim().is_empty())
}

fn absolute_path(path: PathBuf, cwd: &Path) -> Option<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(&path) {
        return None;
    }
    let source = if path.is_absolute() {
        path
    } else {
        if crate::io::is_rooted_but_not_absolute(cwd) {
            return None;
        }
        cwd.join(path)
    };
    let mut result = PathBuf::new();
    for component in source.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                result.push(component.as_os_str());
            }
        }
    }
    Some(result)
}
