use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::providers::{catalog, kimi_agent_file};
use crate::{Engine, Error, Result};

use super::super::files::read_bounded;
use super::super::package::Package;
use super::super::types::{KIMI_AGENT_ASSET, WORKFLOWS};
use super::support::{absolute_profile_path, profile_home};

pub(super) const MAX_WORKFLOW_SOURCE_BYTES: u64 = 512 * 1024;

pub(super) fn read_source(package: &Package, workflow: &str) -> Result<String> {
    let path = package.workflow(workflow);
    let bytes = read_bounded(&path, MAX_WORKFLOW_SOURCE_BYTES)?;
    String::from_utf8(bytes).map_err(|error| Error::InvalidState {
        path,
        message: error.to_string(),
    })
}

pub(super) fn read_kimi_agent_source(package: &Package) -> Result<String> {
    read_kimi_agent_source_from_path(&package.asset(KIMI_AGENT_ASSET))
}

pub(super) fn read_kimi_agent_source_from_path(path: &Path) -> Result<String> {
    super::super::package::validate_kimi_agent(path)?;
    let bytes = read_bounded(path, MAX_WORKFLOW_SOURCE_BYTES)?;
    String::from_utf8(bytes).map_err(|error| Error::InvalidState {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub(super) fn render_workflow(engine: Engine, workflow: &str, source: &str) -> Result<String> {
    if !WORKFLOWS.contains(&workflow) {
        return Err(Error::InvalidArgument(format!(
            "unknown Neomax workflow {workflow}"
        )));
    }
    let body = source
        .replace("{{ENGINE}}", engine.as_str())
        .replace("{{PROVIDER}}", provider_label(engine));
    let provider = format!("Provider entry: {}\n\n", provider_label(engine));
    if let Some(frontmatter) = body.strip_prefix("---\n") {
        if let Some(end) = frontmatter.find("\n---") {
            let split = 4 + end + 4;
            return Ok(format!(
                "{}\n\n{}{}",
                &body[..split],
                provider,
                &body[split..]
            ));
        }
    }
    Ok(format!("{}{}", provider, body))
}

fn provider_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "Claude",
        Engine::Codex => "Codex",
        Engine::Opencode => "OpenCode",
        Engine::Kimi => "Kimi",
        Engine::Grok => "Grok",
    }
}

pub(super) fn discover_profiles(home: &Path) -> BTreeMap<Engine, Vec<PathBuf>> {
    Engine::ALL
        .into_iter()
        .map(|engine| (engine, discover_engine_profiles(engine, home)))
        .collect()
}

fn discover_engine_profiles(engine: Engine, home: &Path) -> Vec<PathBuf> {
    let provider = catalog::spec(engine);
    let mut paths = env::var_os(&provider.profile_env)
        .map(|value| {
            env::split_paths(&value)
                .filter_map(|path| absolute_profile_path(path, home))
                .filter(|path| path.starts_with(home))
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| {
            let mut values = vec![home.join(&provider.default_profile_dir)];
            if let Ok(entries) = fs::read_dir(home) {
                let mut extras = entries
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with(&provider.account_prefix))
                    })
                    .collect::<Vec<_>>();
                extras.sort();
                values.extend(extras);
            }
            values
        });
    if let Some(path) = env::var_os(&provider.orchestrator_env)
        .map(PathBuf::from)
        .and_then(|path| absolute_profile_path(path, home))
        .filter(|path| path.starts_with(home))
    {
        paths.push(path);
    } else {
        let path = home.join(&provider.orchestrator_dir);
        if path.is_dir() {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

pub(super) fn workflow_target(
    engine: Engine,
    profile: &Path,
    workflow: &str,
    home: &Path,
    allow_process_environment: bool,
) -> PathBuf {
    match engine {
        Engine::Claude => profile.join("commands").join(workflow),
        Engine::Codex => profile.join("prompts").join(workflow),
        Engine::Opencode => {
            let config = opencode_config_dir(home, allow_process_environment);
            config.join("opencode/commands").join(workflow)
        }
        Engine::Kimi => profile
            .join("skills")
            .join(workflow.strip_suffix(".md").unwrap_or(workflow))
            .join("SKILL.md"),
        Engine::Grok => profile.join("commands").join(workflow),
    }
}

fn opencode_config_dir(home: &Path, allow_process_environment: bool) -> PathBuf {
    let process_home = profile_home().ok();
    let configured = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    opencode_config_dir_from(
        home,
        allow_process_environment,
        process_home.as_deref(),
        configured.as_deref(),
    )
}

fn opencode_config_dir_from(
    home: &Path,
    allow_process_environment: bool,
    process_home: Option<&Path>,
    configured: Option<&Path>,
) -> PathBuf {
    if allow_process_environment
        && process_home.is_some_and(|process_home| same_path(process_home, home))
    {
        if let Some(configured) = configured.filter(|path| path.is_absolute()) {
            return configured.to_path_buf();
        }
    }
    home.join(".config")
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
        || (cfg!(windows)
            && left
                .to_string_lossy()
                .replace('\\', "/")
                .eq_ignore_ascii_case(&right.to_string_lossy().replace('\\', "/")))
}

pub(super) fn kimi_agent_target(profile: &Path) -> PathBuf {
    kimi_agent_file(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_profile_home_uses_its_own_config_root() {
        let root = std::env::current_dir().unwrap().join("fixture-home");
        let process_home = std::env::current_dir().unwrap().join("process-home");
        let configured = process_home.join(".config");
        let target = workflow_target(
            Engine::Opencode,
            &root.join(".opencode"),
            "neomax.md",
            &root,
            false,
        );
        assert_eq!(target, root.join(".config/opencode/commands/neomax.md"));
        assert_eq!(
            opencode_config_dir_from(&root, false, Some(&process_home), Some(&configured)),
            root.join(".config")
        );
    }

    #[test]
    fn process_home_honors_absolute_xdg_config_root() {
        let root = std::env::current_dir().unwrap().join("process-home");
        let configured = std::env::current_dir().unwrap().join("xdg-config");
        assert_eq!(
            opencode_config_dir_from(&root, true, Some(&root), Some(&configured)),
            configured
        );
    }
}
