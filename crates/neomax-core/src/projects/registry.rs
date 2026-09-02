use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::atomic::{
    read_json_or_default, read_json_or_default_on_missing, update_json_locked_strict,
    with_exclusive_lock, write_json_atomic,
};
use crate::{Error, Result};

use super::discovery::discover_repositories;
use super::orientation::ProjectOrientation;
use super::slug::project_slug;
use super::types::Project;

pub struct ProjectRegistry {
    state: PathBuf,
    local_seed: Option<PathBuf>,
}

impl ProjectRegistry {
    pub fn new(state: impl Into<PathBuf>, local_seed: Option<PathBuf>) -> Self {
        Self {
            state: state.into(),
            local_seed,
        }
    }

    pub fn load(&self) -> BTreeMap<String, Project> {
        let mut projects: BTreeMap<String, Project> = self
            .local_seed
            .as_deref()
            .map(read_json_or_default)
            .unwrap_or_default();
        projects.extend(read_json_or_default::<BTreeMap<String, Project>>(
            &self.state,
        ));
        projects
    }

    pub fn try_load(&self) -> Result<BTreeMap<String, Project>> {
        let mut projects = match self.local_seed.as_deref() {
            Some(path) => read_json_or_default_on_missing(path)?,
            None => BTreeMap::new(),
        };
        projects.extend(read_json_or_default_on_missing::<BTreeMap<String, Project>>(&self.state)?);
        Ok(projects)
    }

    pub fn project_of(&self, path: &Path) -> Option<String> {
        let path = absolute(path).ok()?;
        let projects = self.load();
        projects
            .iter()
            .filter_map(|(name, project)| {
                let root = absolute(&project.root).ok()?;
                (path == root || path.starts_with(&root))
                    .then_some((name, root.components().count()))
            })
            .max_by_key(|(_, component_count)| *component_count)
            .map(|(name, _)| name.clone())
            .or_else(|| project_for_repository_name(&projects, &path))
    }

    pub fn orientation_of(&self, path: &Path) -> Option<ProjectOrientation> {
        let path = absolute(path).ok()?;
        let (name, project, _) = self
            .load()
            .into_iter()
            .filter_map(|(name, project)| {
                let root = absolute(&project.root).ok()?;
                (path == root || path.starts_with(&root)).then_some((
                    name,
                    project,
                    root.components().count(),
                ))
            })
            .max_by_key(|(_, _, component_count)| *component_count)?;
        Some(ProjectOrientation::from_project(name, &project))
    }

    pub fn register(&self, name: &str, mut project: Project, overwrite: bool) -> Result<String> {
        let name = project_slug(name);
        project.root = absolute(&project.root)?;
        let seeds = self.load_seed_strict()?;
        update_json_locked_strict::<BTreeMap<String, Project>, _>(
            &self.state,
            &lock_path(&self.state),
            |state| {
                let mut all = seeds.clone();
                all.extend(std::mem::take(state));
                validate_registration(&all, &name, &project, overwrite)?;
                *state = all;
                state.insert(name.clone(), project);
                Ok(())
            },
        )?;
        Ok(name)
    }

    pub fn unregister(&self, name: &str) -> Result<Option<Project>> {
        let name = project_slug(name);
        let seeds = self.load_seed_strict()?;
        let mut removed = None;
        with_exclusive_lock(&lock_path(&self.state), || {
            let state = read_json_or_default_on_missing::<BTreeMap<String, Project>>(&self.state)?;
            let mut all = seeds.clone();
            all.extend(state);
            removed = all.remove(&name);
            if removed.is_some() {
                write_json_atomic(&self.state, &all)?;
            }
            Ok(())
        })?;
        Ok(removed)
    }

    fn load_seed_strict(&self) -> Result<BTreeMap<String, Project>> {
        match self.local_seed.as_deref() {
            Some(path) => read_json_or_default_on_missing(path),
            None => Ok(BTreeMap::new()),
        }
    }

    pub fn ensure_launch_project(
        &self,
        root: &Path,
        home: &Path,
        preferred_name: Option<&str>,
        now: i64,
    ) -> Result<Option<String>> {
        let root = absolute(root)?;
        let home = absolute(home)?;
        if root.parent().is_none() || root == home || !root.is_dir() {
            return Ok(self.project_of(&root));
        }
        if let Some(existing) = self.project_of(&root) {
            return Ok(Some(existing));
        }
        let projects = self.load();
        let base = project_slug(preferred_name.unwrap_or_else(|| file_name(&root)));
        let name = unique_name(&projects, &base, &root);
        let prefix = unique_prefix(&projects, &name, &root);
        let mut project = Project::portable(root.clone(), prefix, now);
        project.repos = discover_repositories(&root);
        project.description = Some(format!("{name} project rooted at {}", root.display()));
        self.register(&name, project, false).map(Some)
    }
}

fn validate_registration(
    projects: &BTreeMap<String, Project>,
    name: &str,
    project: &Project,
    overwrite: bool,
) -> Result<()> {
    if projects.contains_key(name) && !overwrite {
        return Err(Error::Conflict(format!(
            "a project named {name} is already registered"
        )));
    }
    for (other, current) in projects {
        if other == name {
            continue;
        }
        let current_root = absolute(&current.root)?;
        if project.root == current_root
            || project.root.starts_with(&current_root)
            || current_root.starts_with(&project.root)
        {
            return Err(Error::Conflict(format!(
                "project root {} overlaps {other} at {}",
                project.root.display(),
                current_root.display()
            )));
        }
    }
    Ok(())
}

fn project_for_repository_name(
    projects: &BTreeMap<String, Project>,
    path: &Path,
) -> Option<String> {
    let name = path.file_name()?;
    let mut matches = projects.iter().filter_map(|(project_name, project)| {
        project
            .repos
            .iter()
            .any(|repo| repo.as_os_str() == name)
            .then_some(project_name.clone())
    });
    let owner = matches.next()?;
    matches.next().is_none().then_some(owner)
}

fn unique_name(projects: &BTreeMap<String, Project>, base: &str, root: &Path) -> String {
    if projects
        .get(base)
        .is_none_or(|project| same_path(&project.root, root))
    {
        return base.into();
    }
    format!("{base}{}", hash_prefix(root, 6))
}

fn unique_prefix(projects: &BTreeMap<String, Project>, name: &str, root: &Path) -> String {
    let used = projects
        .values()
        .filter_map(|project| project.branch_prefix.clone())
        .collect::<BTreeSet<_>>();
    let prefix = name.chars().take(4).collect::<String>();
    if !used.contains(&prefix) {
        return prefix;
    }
    format!(
        "{}{}",
        name.chars().take(3).collect::<String>(),
        hash_prefix(root, 3)
    )
}

fn hash_prefix(path: &Path, length: usize) -> String {
    let hash = format!("{:x}", Sha256::digest(path.to_string_lossy().as_bytes()));
    hash.chars().take(length).collect()
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidArgument("project root is empty".into()));
    }
    if crate::io::is_rooted_but_not_absolute(path) {
        return Err(Error::InvalidArgument(format!(
            "project path must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let path = normalize_lexically(&path);
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }

    // A project query commonly names a file that does not exist yet. Resolve
    // the existing prefix so symlink aliases such as /var and /private/var
    // remain equivalent, then append the normalized non-existent suffix.
    let mut existing = path.clone();
    let mut suffix = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        suffix.push(name.to_os_string());
        if !existing.pop() {
            break;
        }
    }
    let mut resolved = existing.canonicalize().unwrap_or(existing);
    for name in suffix.iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn same_path(left: &Path, right: &Path) -> bool {
    if crate::io::is_rooted_but_not_absolute(left)
        || crate::io::is_rooted_but_not_absolute(right)
    {
        return false;
    }
    match (absolute(left), absolute(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => normalize_lexically(left) == normalize_lexically(right),
    }
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project")
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}
