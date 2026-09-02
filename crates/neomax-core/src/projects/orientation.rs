use std::fs;
use std::path::{Component, Path, PathBuf};

use super::types::Project;

const MAX_OPENER_BYTES: usize = 8 * 1024;

/// Product-safe project facts used by an interactive orchestrator opener.
///
/// Project state may contain user-selected paths, so this type keeps the
/// configured locations separate from content. Only the explicitly registered
/// opener is eligible for bounded content loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectOrientation {
    pub name: String,
    pub root: PathBuf,
    pub repos: Vec<PathBuf>,
    pub branch_prefix: Option<String>,
    pub brain: Option<PathBuf>,
    pub agents: Option<PathBuf>,
    pub orch_brain: Option<PathBuf>,
    pub planning: Option<PathBuf>,
    pub opener_content: Option<String>,
}

impl ProjectOrientation {
    pub fn from_project(name: impl Into<String>, project: &Project) -> Self {
        let name = name.into();
        let opener_content = project
            .opener
            .as_deref()
            .and_then(|path| read_opener(&project.root, path));
        Self {
            name,
            root: project.root.clone(),
            repos: project.repos.clone(),
            branch_prefix: project.branch_prefix.clone(),
            brain: project.brain.clone(),
            agents: project.agents.clone(),
            orch_brain: project.orch_brain.clone(),
            planning: project.planning.clone(),
            opener_content,
        }
    }

    pub fn relative_location(&self, path: Option<&Path>) -> Option<String> {
        let path = path?;
        let root = self.root.canonicalize().ok()?;
        safe_project_path(&self.root, path).and_then(|candidate| {
            candidate
                .strip_prefix(&root)
                .ok()
                .and_then(|relative| (!relative.as_os_str().is_empty()).then_some(relative))
                .map(portable_relative_location)
        })
    }
}

fn portable_relative_location(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Resolve a configured project path without permitting it to escape the
/// registered root, including through a symlink.
pub fn safe_project_path(root: &Path, configured: &Path) -> Option<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(root)
        || crate::io::is_rooted_but_not_absolute(configured)
    {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let candidate = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        root.join(configured)
    };
    if configured.is_relative()
        && configured
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    let candidate = lexical_path(&candidate);
    let mut existing = candidate.clone();
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?.to_path_buf();
    }
    let existing = existing.canonicalize().ok()?;
    if !existing.starts_with(&root) {
        return None;
    }
    suffix.reverse();
    Some(
        suffix
            .into_iter()
            .fold(existing, |path, component| path.join(component)),
    )
}

fn lexical_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn read_opener(root: &Path, configured: &Path) -> Option<String> {
    let path = safe_project_path(root, configured)?;
    if !path.is_file() {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() > MAX_OPENER_BYTES {
        return None;
    }
    let content = String::from_utf8(bytes).ok()?;
    product_safe_text(&content).then_some(content.trim().to_owned())
}

fn product_safe_text(value: &str) -> bool {
    value.chars().all(|character| {
        character == '\n' || character == '\r' || character == '\t' || !character.is_control()
    }) && !value.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        [
            "api_key",
            "api-key",
            "auth_token",
            "auth-token",
            "access_token",
            "access-token",
            "password",
            "private_key",
            "private-key",
            "bearer ",
            "cookie:",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn reads_only_a_bounded_registered_opener() {
        let temp = tempfile::tempdir().unwrap();
        let opener = temp.path().join("docs/opener.md");
        fs::create_dir_all(opener.parent().unwrap()).unwrap();
        fs::write(&opener, "Product rules\nVerify before handoff\n").unwrap();
        let project = Project {
            root: temp.path().to_path_buf(),
            opener: Some(PathBuf::from("docs/opener.md")),
            ..Project::portable(temp.path().to_path_buf(), "proj".into(), 1)
        };
        let orientation = ProjectOrientation::from_project("fixture", &project);
        assert_eq!(
            orientation.opener_content.as_deref(),
            Some("Product rules\nVerify before handoff")
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_escape_symlink_and_secret_like_opener_content() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.md");
        fs::write(&secret, "API_KEY=not-for-agents").unwrap();
        let link = temp.path().join("link.md");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let project = Project {
            root: temp.path().to_path_buf(),
            opener: Some(PathBuf::from("link.md")),
            ..Project::portable(temp.path().to_path_buf(), "proj".into(), 1)
        };
        assert!(
            ProjectOrientation::from_project("fixture", &project)
                .opener_content
                .is_none()
        );
        assert!(safe_project_path(temp.path(), Path::new("../secret.md")).is_none());
    }

    #[test]
    fn reports_only_relative_locations_under_the_registered_root() {
        let temp = tempfile::tempdir().unwrap();
        let project = ProjectOrientation::from_project(
            "fixture",
            &Project::portable(temp.path().to_path_buf(), "proj".into(), 1),
        );
        assert_eq!(
            project.relative_location(Some(Path::new("CLAUDE.md"))),
            Some("CLAUDE.md".into())
        );
        assert_eq!(
            project.relative_location(Some(&temp.path().join("CLAUDE.md"))),
            Some("CLAUDE.md".into())
        );
        assert!(
            project
                .relative_location(Some(Path::new("../outside")))
                .is_none()
        );
    }

    #[test]
    fn reports_nested_locations_with_portable_separators() {
        let temp = tempfile::tempdir().unwrap();
        let project = ProjectOrientation::from_project(
            "fixture",
            &Project::portable(temp.path().to_path_buf(), "proj".into(), 1),
        );
        let location = temp
            .path()
            .join("docs")
            .join("neomax-orchestrator")
            .join("ORCHESTRATOR.md");
        assert_eq!(
            project.relative_location(Some(&location)),
            Some("docs/neomax-orchestrator/ORCHESTRATOR.md".into())
        );
    }

    #[cfg(windows)]
    #[test]
    fn rejects_rooted_and_drive_relative_configured_paths() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(&root).unwrap();

        assert!(safe_project_path(&root, Path::new(r"\outside\opener.md")).is_none());
        assert!(safe_project_path(&root, Path::new(r"C:outside\opener.md")).is_none());
    }
}
