use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::issues::{RepositoryCatalog, RepositoryTarget};
use neomax_core::projects::Project;

use crate::context::RuntimeContext;

#[derive(Debug, Clone, Default)]
pub(super) struct LocalCatalog {
    projects: BTreeMap<String, Project>,
}

impl LocalCatalog {
    pub fn from_context(context: &RuntimeContext) -> Self {
        Self {
            projects: context.project_registry().load(),
        }
    }

    pub fn project(&self, name: &str) -> Result<&Project> {
        self.projects
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown project {name:?}"))
    }

    pub fn targets_for(&self, name: &str) -> Result<Vec<RepositoryTarget>> {
        let project = self.project(name)?;
        validate_absolute_path(&project.root, "project root")?;
        let mut targets = Vec::new();
        let repositories = if project.repos.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            project.repos.clone()
        };
        for (index, repository) in repositories.into_iter().enumerate() {
            validate_repository_path(&repository)?;
            let path = if repository.is_absolute() {
                repository
            } else {
                project.root.join(repository)
            };
            validate_absolute_path(&path, "project repository")?;
            if !path.is_dir() {
                return Err(anyhow::anyhow!(
                    "project repository {} does not exist or is not a directory",
                    path.display()
                ));
            }
            let base_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("root");
            let name = if targets
                .iter()
                .any(|target: &RepositoryTarget| target.name == base_name)
            {
                format!("{base_name}-{index}")
            } else {
                base_name.to_owned()
            };
            targets.push(RepositoryTarget::new(name, path));
        }
        if targets.is_empty() {
            return Err(anyhow::anyhow!("project {name:?} has no repositories"));
        }
        Ok(targets)
    }
}

fn validate_repository_path(path: &Path) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        return Err(anyhow::anyhow!(
            "project repository must not be rooted without an absolute prefix: {}",
            path.display()
        ));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(anyhow::anyhow!(
            "project repository cannot contain parent-directory traversal: {}",
            path.display()
        ));
    }
    Ok(())
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<()> {
    validate_repository_path(path)?;
    if !path.is_absolute() {
        return Err(anyhow::anyhow!(
            "{label} must be absolute: {}",
            path.display()
        ));
    }
    Ok(())
}

impl RepositoryCatalog for LocalCatalog {
    fn repositories(&self, project: &str) -> neomax_core::Result<Vec<RepositoryTarget>> {
        self.targets_for(project)
            .map_err(|error| neomax_core::Error::InvalidArgument(error.to_string()))
    }
}

pub(super) fn project_name(context: &RuntimeContext, value: Option<&str>) -> Result<String> {
    value
        .map(str::to_owned)
        .or_else(|| context.project_for_cwd())
        .ok_or_else(|| anyhow::anyhow!("not in a registered project; pass --project"))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn project_roots_require_absolute_non_traversing_paths() {
        let temp = tempfile::tempdir().expect("temporary root");
        assert!(validate_absolute_path(temp.path(), "project root").is_ok());
        assert!(validate_absolute_path(Path::new("../project"), "project root").is_err());
        assert!(validate_absolute_path(Path::new("project"), "project root").is_err());
    }

    #[test]
    fn repository_labels_allow_safe_relative_paths() {
        assert!(validate_repository_path(Path::new("repo")).is_ok());
        assert!(validate_repository_path(Path::new("nested/repo")).is_ok());
        assert!(validate_repository_path(Path::new("../repo")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn project_and_repository_paths_reject_windows_partial_roots() {
        for path in [Path::new(r"\project"), Path::new(r"C:project")] {
            assert!(validate_repository_path(path).is_err());
        }
    }
}
