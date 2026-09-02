use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use neomax_core::projects::Project;
use neomax_core::runs::{RunRecord, SystemProcessProbe, effective_status};

use crate::model::WorktreeView;

use super::FilesystemPortalSource;

const MAX_WORKTREES: usize = 10_000;

pub(crate) fn read_worktrees(
    source: &FilesystemPortalSource,
    runs: &[RunRecord],
    projects: &BTreeMap<String, Project>,
) -> Result<(Vec<WorktreeView>, usize)> {
    let mut rows = BTreeMap::<PathBuf, WorktreeView>::new();
    let probe = SystemProcessProbe;
    let mut skipped = 0;

    for run in runs {
        let Some(path) = run.worktree.clone() else {
            continue;
        };
        let id = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&run.id)
            .to_owned();
        rows.entry(path.clone()).or_insert_with(|| WorktreeView {
            id,
            path: path.clone(),
            exists: path.is_dir(),
            run_id: Some(run.id.clone()),
            status: Some(effective_status(run, &probe).as_str().into()),
            state: run.worktree_state.clone(),
            repository: run.repo.clone(),
            branch: run.branch.clone(),
            project: run
                .project
                .clone()
                .or_else(|| project_for_path(projects, &path)),
            files: run.files_touched.clone(),
        });
    }

    let root_metadata = match fs::symlink_metadata(&source.paths.worktrees) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((rows.into_values().collect(), skipped));
        }
        Err(error) => return Err(error.into()),
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Ok((rows.into_values().collect(), skipped.saturating_add(1)));
    }
    let entries = fs::read_dir(&source.paths.worktrees)?;
    for entry in entries {
        if rows.len() >= MAX_WORKTREES {
            skipped = skipped.saturating_add(1);
            break;
        }
        let Ok(entry) = entry else {
            skipped = skipped.saturating_add(1);
            continue;
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                skipped = skipped.saturating_add(1);
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            skipped = skipped.saturating_add(1);
            continue;
        }
        rows.entry(path.clone()).or_insert_with(|| WorktreeView {
            id: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("worktree")
                .into(),
            path,
            exists: true,
            ..WorktreeView::default()
        });
    }
    Ok((rows.into_values().collect(), skipped))
}

fn project_for_path(projects: &BTreeMap<String, Project>, path: &Path) -> Option<String> {
    projects
        .iter()
        .filter(|(_, project)| path == project.root || path.starts_with(&project.root))
        .max_by_key(|(_, project)| project.root.components().count())
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::Engine;

    #[test]
    fn combines_referenced_and_unowned_worktrees_without_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let root = state.join("worktrees");
        fs::create_dir_all(root.join("unowned")).unwrap();
        let referenced = root.join("run-1");
        fs::create_dir_all(&referenced).unwrap();
        let mut run = RunRecord::new(
            "run-1",
            Engine::Claude,
            "claude-fable-5[1m]",
            "work",
            "/fixture/profile",
            "/fixture/project",
            1,
        );
        run.worktree = Some(referenced.clone());
        run.worktree_state = Some("has_changes".into());
        run.files_touched = vec!["src/lib.rs".into()];
        let source = FilesystemPortalSource::new(temp.path(), state);
        let (rows, skipped) = read_worktrees(&source, &[run], &BTreeMap::new()).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(rows.len(), 2);
        let row = rows.iter().find(|row| row.run_id.is_some()).unwrap();
        assert_eq!(row.id, "run-1");
        assert_eq!(row.state.as_deref(), Some("has_changes"));
        assert_eq!(row.files, ["src/lib.rs"]);
        assert!(
            rows.iter()
                .any(|row| row.id == "unowned" && row.run_id.is_none())
        );
    }

    #[test]
    fn missing_worktree_root_preserves_referenced_vanished_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let mut run = RunRecord::new(
            "run-1",
            Engine::Codex,
            "gpt-5.6-sol",
            "work",
            "/fixture/profile",
            "/fixture/project",
            1,
        );
        run.worktree = Some(state.join("worktrees/run-1"));
        let source = FilesystemPortalSource::new(temp.path(), &state);
        let (rows, skipped) = read_worktrees(&source, &[run], &BTreeMap::new()).unwrap();
        assert_eq!(skipped, 0);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].exists);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_worktree_root_is_ignored_without_following_external_state() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let external = temp.path().join("external");
        fs::create_dir_all(&external).unwrap();
        fs::create_dir_all(&state).unwrap();
        std::os::unix::fs::symlink(&external, state.join("worktrees")).unwrap();
        let source = FilesystemPortalSource::new(temp.path(), &state);
        let (rows, skipped) = read_worktrees(&source, &[], &BTreeMap::new()).unwrap();
        assert!(rows.is_empty());
        assert_eq!(skipped, 1);
    }
}
