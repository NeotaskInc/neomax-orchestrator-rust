use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use neomax_core::projects::{Project, ProjectRegistry, discover_repositories};
use neomax_core::{Error, Result};

use crate::config::RuntimeConfig;
use crate::git::{GitRunner, args};
use crate::paths::{canonical_or_lexical, relative_repo, repository_label};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySpec {
    pub relative: PathBuf,
    pub root: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub name: String,
    pub root: PathBuf,
    pub branch_prefix: String,
    pub worktree_root: PathBuf,
    pub repositories: Vec<RepositorySpec>,
}

pub fn resolve<G: GitRunner>(
    config: &RuntimeConfig,
    cwd: &Path,
    git: &G,
) -> Result<ProjectContext> {
    config.validate()?;
    let cwd = canonical_or_lexical(cwd)?;
    let registry = ProjectRegistry::new(config.state_file(), None);
    let projects = registry.try_load()?;
    let explicit_root = config
        .project_dir
        .as_deref()
        .map(|path| config.expand_from(path, &cwd))
        .transpose()?
        .map(|path| canonical_or_lexical(&path))
        .transpose()?;
    let selected = if let Some(root) = explicit_root.as_deref() {
        longest_project(&projects, root)?
    } else {
        longest_project(&projects, &cwd)?.or_else(|| {
            registry
                .project_of(&cwd)
                .and_then(|name| projects.get_key_value(&name))
                .map(|(name, project)| (name.as_str(), project))
        })
    };
    let selected = selected
        .map(|(name, project)| {
            canonical_or_lexical(&project.root).map(|root| (name, project, root))
        })
        .transpose()?;
    let (name, root, project) = match selected {
        Some((name, project, project_root))
            if explicit_root.is_none()
                || explicit_root.as_deref() == Some(project_root.as_path()) =>
        {
            (name.to_owned(), project_root, Some(project.clone()))
        }
        _ => {
            let root = explicit_root
                .unwrap_or_else(|| git_repository_root(git, &cwd).unwrap_or_else(|_| cwd.clone()));
            let name = neomax_core::projects::project_slug(
                root.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("project"),
            );
            (name, root, None)
        }
    };
    let relative_repositories = config.repos.clone().unwrap_or_else(|| {
        project
            .as_ref()
            .map(|value| value.repos.clone())
            .unwrap_or_else(|| {
                if root.join(".git").exists() {
                    vec![PathBuf::from(".")]
                } else {
                    discover_repositories(&root)
                }
            })
    });
    let mut repositories = Vec::new();
    let mut labels = BTreeMap::new();
    for relative in relative_repositories {
        relative_repo(&relative)?;
        let label = repository_label(&root, &relative)?;
        if let Some(previous) = labels.insert(label.clone(), relative.clone()) {
            return Err(Error::Conflict(format!(
                "repository labels collide: {} and {} both map to {}",
                previous.display(),
                relative.display(),
                label
            )));
        }
        repositories.push(RepositorySpec {
            root: canonical_or_lexical(&root.join(&relative))?,
            relative,
            label,
        });
    }
    let branch_prefix = config
        .branch_prefix
        .clone()
        .or_else(|| {
            project
                .as_ref()
                .and_then(|value| value.branch_prefix.clone())
        })
        .unwrap_or_else(|| name.chars().take(4).collect::<String>());
    let branch_prefix = if branch_prefix.is_empty() {
        "proj".to_owned()
    } else {
        branch_prefix
    };
    let worktree_root = config
        .worktree_root
        .as_deref()
        .map(|path| config.expand_from(path, &cwd))
        .transpose()?
        .unwrap_or_else(|| config.home.join("coordinated-worktrees").join(&name));
    Ok(ProjectContext {
        name,
        root,
        branch_prefix,
        worktree_root: canonical_or_lexical(&worktree_root)?,
        repositories,
    })
}

fn longest_project<'a>(
    projects: &'a BTreeMap<String, Project>,
    path: &Path,
) -> Result<Option<(&'a str, &'a Project)>> {
    let mut selected = None;
    for (name, project) in projects {
        let root = canonical_or_lexical(&project.root)?;
        if path == root || path.starts_with(&root) {
            let depth = project.root.components().count();
            if selected.is_none_or(|(_, _, selected_depth)| depth > selected_depth) {
                selected = Some((name.as_str(), project, depth));
            }
        }
    }
    Ok(selected.map(|(name, project, _)| (name, project)))
}

pub fn is_git_repository<G: GitRunner>(git: &G, path: &Path) -> Result<bool> {
    match git.run(path, &args(["rev-parse", "--show-toplevel"])) {
        Ok(result) => Ok(result.success),
        Err(Error::Io(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

fn git_repository_root<G: GitRunner>(git: &G, path: &Path) -> Result<PathBuf> {
    let output = git.run(path, &args(["rev-parse", "--show-toplevel"]))?;
    if !output.success {
        return Err(Error::NotFound(format!(
            "no Git repository at {}",
            path.display()
        )));
    }
    let value = output.stdout_text();
    if value.is_empty() {
        return Err(Error::Message(
            "Git returned an empty repository root".into(),
        ));
    }
    canonical_or_lexical(Path::new(&value))
}
