use std::env;
use std::path::{Path, PathBuf};

use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::{Error, Result};

use crate::command::Cli;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub home: PathBuf,
    pub project_dir: Option<PathBuf>,
    pub repos: Option<Vec<PathBuf>>,
    pub branch_prefix: Option<String>,
    pub worktree_root: Option<PathBuf>,
    pub dry_run: bool,
    pub json: bool,
}

impl RuntimeConfig {
    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let home = cli
            .home
            .clone()
            .or_else(|| env::var_os("NEOMAX_HOME").map(PathBuf::from))
            .or_else(|| {
                env::var_os("HOME")
                    .or_else(|| env::var_os("USERPROFILE"))
                    .map(|value| PathBuf::from(value).join(".neomax"))
            })
            .ok_or_else(|| Error::InvalidArgument("HOME is not set".into()))?;
        let repos = match cli.repos.as_deref() {
            Some(value) => Some(parse_repositories(value)?),
            None => env::var("NEOMAX_REPOS")
                .ok()
                .map(|value| parse_repositories(&value))
                .transpose()?,
        };
        let config = Self {
            home,
            project_dir: cli
                .project_dir
                .clone()
                .or_else(|| env::var_os("NEOMAX_PROJECT_DIR").map(PathBuf::from)),
            repos,
            branch_prefix: cli
                .branch_prefix
                .clone()
                .or_else(|| env::var("NEOMAX_BRANCH_PREFIX").ok()),
            worktree_root: cli
                .worktree_root
                .clone()
                .or_else(|| env::var_os("NEOMAX_WORKTREE_ROOT").map(PathBuf::from)),
            dry_run: cli.dry_run || env::var_os("NEOMAX_DRY_RUN").is_some(),
            json: cli.json,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn state_file(&self) -> PathBuf {
        self.home.join("projects.json")
    }

    pub fn expand_from(&self, path: &Path, cwd: &Path) -> Result<PathBuf> {
        self.validate()?;
        if is_rooted_but_not_absolute(path) {
            return Err(Error::InvalidArgument(format!(
                "path must not be rooted without an absolute prefix: {}",
                path.display()
            )));
        }
        if !path.is_absolute() && !cwd.is_absolute() {
            return Err(Error::InvalidArgument(format!(
                "working directory must be absolute: {}",
                cwd.display()
            )));
        }
        Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        })
    }

    pub(crate) fn validate(&self) -> Result<()> {
        validate_root(&self.home, "Neomax home")?;
        if let Some(path) = self.project_dir.as_deref() {
            validate_path(path, "project directory")?;
        }
        if let Some(paths) = self.repos.as_deref() {
            for path in paths {
                validate_path(path, "repository path")?;
            }
        }
        if let Some(path) = self.worktree_root.as_deref() {
            validate_path(path, "worktree root")?;
        }
        Ok(())
    }
}

fn validate_root(path: &Path, label: &str) -> Result<()> {
    validate_path(path, label)?;
    if !path.is_absolute() {
        return Err(Error::InvalidArgument(format!(
            "{label} must be absolute: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_path(path: &Path, label: &str) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        return Err(Error::InvalidArgument(format!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    Ok(())
}

fn parse_repositories(value: &str) -> Result<Vec<PathBuf>> {
    let repositories = value
        .split([',', ' ', '\t', '\n'])
        .filter(|item| !item.trim().is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if repositories.is_empty() {
        return Err(Error::InvalidArgument("repository list is empty".into()));
    }
    for repository in &repositories {
        validate_path(repository, "repository path")?;
    }
    Ok(repositories)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn rooted_and_drive_relative_paths_are_rejected_before_resolution() {
        let config = RuntimeConfig {
            home: PathBuf::from(r"C:\state"),
            project_dir: None,
            repos: None,
            branch_prefix: None,
            worktree_root: None,
            dry_run: false,
            json: false,
        };
        let cwd = Path::new(r"C:\workspace");

        for path in [Path::new(r"\workspace"), Path::new(r"C:workspace")] {
            assert!(config.expand_from(path, cwd).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn config_validation_rejects_rooted_and_drive_relative_overrides() {
        let cwd = PathBuf::from(r"C:\workspace");
        for project_dir in [PathBuf::from(r"\project"), PathBuf::from(r"C:project")] {
            let config = RuntimeConfig {
                home: cwd.clone(),
                project_dir: Some(project_dir),
                repos: None,
                branch_prefix: None,
                worktree_root: None,
                dry_run: false,
                json: false,
            };
            assert!(config.validate().is_err());
        }
    }
}
