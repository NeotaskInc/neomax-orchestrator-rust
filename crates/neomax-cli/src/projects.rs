use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::projects::{Project, project_slug};

use crate::context::RuntimeContext;
use crate::error;
use crate::output;

pub fn list(context: &RuntimeContext, _args: &[String]) -> Result<()> {
    output::json(&context.project_registry().load())
}

pub fn register(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let options = error::usage(RegisterOptions::parse(args))?;
    let registry = context.project_registry();
    if let Some(name) = options.unregister {
        let name = project_slug(&name);
        if registry.unregister(&name)?.is_some() {
            println!("project-unregister: removed '{name}' (its files on disk are left untouched)");
            return Ok(());
        }
        bail!("no registered project named '{name}'");
    }

    let root = match options.root.as_deref() {
        Some(value) => {
            validate_path(Path::new(value), "project root")?;
            context.resolve_path(value)
        }
        None => context.cwd.clone(),
    };
    validate_absolute_path(&root, "project root")?;
    if root.parent().is_none() {
        return Err(error::usage_error(anyhow::anyhow!(
            "project root must not be the filesystem root"
        )));
    }
    if root == context.paths.home {
        return Err(error::usage_error(anyhow::anyhow!(
            "project root must not be the user's home directory"
        )));
    }
    let name = project_slug(
        options
            .name
            .as_deref()
            .or_else(|| root.file_name().and_then(|value| value.to_str()))
            .unwrap_or("project"),
    );
    let branch_prefix = options.prefix.unwrap_or_else(|| {
        let prefix = name.chars().take(4).collect::<String>();
        if prefix.is_empty() {
            "prj".into()
        } else {
            prefix
        }
    });
    let repos = options
        .repos
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|repo| !repo.is_empty())
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .filter(|repos| !repos.is_empty())
        .unwrap_or_else(|| vec![PathBuf::from(".")]);
    for repository in &repos {
        validate_path(repository, "project repository")?;
    }
    let project = Project {
        root: root.clone(),
        repos,
        branch_prefix: Some(branch_prefix.clone()),
        brain: Some(configured_path(
            options.brain,
            "CLAUDE.md",
            "project brain",
        )?),
        agents: Some(configured_path(
            options.agents,
            "AGENTS.md",
            "project agents",
        )?),
        orch_brain: Some(configured_path(
            options.orch_brain,
            "docs/neomax-orchestrator/ORCHESTRATOR.md",
            "project orchestrator guide",
        )?),
        opener: Some(configured_path(
            options.opener,
            "docs/neomax-orchestrator/ORCHESTRATOR_OPENER.md",
            "project opener",
        )?),
        planning: Some(configured_path(
            options.planning,
            "docs/neomax-orchestrator",
            "project planning directory",
        )?),
        description: options
            .description
            .or_else(|| Some(format!("{name} project rooted at {}", root.display()))),
        created_at: Some(context.now),
        auto_registered: false,
        extra: Default::default(),
    };
    let name = registry.register(&name, project, options.force)?;
    println!(
        "project-register: '{name}' registered -> root {}, branch namespace `{branch_prefix}/`. Runs/sessions/sub-agents under this root now tag `project={name}`.",
        root.display()
    );
    Ok(())
}

#[derive(Debug, Default)]
struct RegisterOptions {
    name: Option<String>,
    root: Option<String>,
    prefix: Option<String>,
    repos: Option<String>,
    description: Option<String>,
    brain: Option<String>,
    agents: Option<String>,
    orch_brain: Option<String>,
    opener: Option<String>,
    planning: Option<String>,
    unregister: Option<String>,
    force: bool,
}

impl RegisterOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            if flag == "--force" {
                options.force = true;
                index += 1;
                continue;
            }
            let (flag, inline) = split_flag(flag);
            let value = if let Some(value) = inline {
                value.to_owned()
            } else {
                index += 1;
                args.get(index)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))?
            };
            match flag {
                "--name" => options.name = Some(value),
                "--root" => options.root = Some(value),
                "--prefix" => options.prefix = Some(value),
                "--repos" => options.repos = Some(value),
                "--desc" => options.description = Some(value),
                "--brain" => options.brain = Some(value),
                "--agents" => options.agents = Some(value),
                "--orch-brain" => options.orch_brain = Some(value),
                "--opener" => options.opener = Some(value),
                "--planning" => options.planning = Some(value),
                "--unregister" => options.unregister = Some(value),
                _ => bail!("unknown project option {flag}"),
            }
            index += 1;
        }
        Ok(options)
    }
}

fn split_flag(value: &str) -> (&str, Option<&str>) {
    value
        .split_once('=')
        .map_or((value, None), |(flag, value)| (flag, Some(value)))
}

fn configured_path(value: Option<String>, default: &str, label: &str) -> Result<PathBuf> {
    let path = value.map_or_else(|| PathBuf::from(default), PathBuf::from);
    validate_path(&path, label)?;
    Ok(path)
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<()> {
    validate_path(path, label)?;
    if !path.is_absolute() {
        bail!("{label} must be absolute: {}", path.display());
    }
    Ok(())
}

fn validate_path(path: &Path, label: &str) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        bail!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        );
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!(
            "{label} cannot contain parent-directory traversal: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn configured_project_paths_reject_parent_traversal() {
        assert!(validate_path(Path::new("../outside"), "project path").is_err());
        assert!(configured_path(Some("../outside".into()), "default", "project path").is_err());
        assert!(validate_path(Path::new("nested/repository"), "project repository").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn configured_project_paths_reject_windows_partial_roots() {
        for value in [r"\outside", r"C:outside"] {
            assert!(validate_path(Path::new(value), "project path").is_err());
        }
    }
}
