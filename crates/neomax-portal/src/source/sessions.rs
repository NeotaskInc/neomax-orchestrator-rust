use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;

use neomax_core::config::Engine;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::projects::Project;
use neomax_core::runs::RunStatus;
use neomax_core::sessions::{
    DiscoveryContext, FsArtifactSource, SessionKind, SessionRecord, claude, codex, grok, kimi,
    opencode,
};

use super::{FilesystemPortalSource, SESSION_ACTIVITY_WINDOW_SECONDS, state};

pub(crate) fn discover_sessions(
    source: &FilesystemPortalSource,
    days: u32,
    now: i64,
) -> Result<Vec<SessionRecord>> {
    discover_sessions_with_progress(source, days, now, false, &mut |_, _| true)
}

pub(crate) fn discover_sessions_with_progress(
    source: &FilesystemPortalSource,
    days: u32,
    now: i64,
    include_managed: bool,
    report: &mut dyn FnMut(&[SessionRecord], &str) -> bool,
) -> Result<Vec<SessionRecord>> {
    let (runs, _) = super::runs::read_records(&source.paths.runs)?;
    let orchestrators = if include_managed {
        read_orchestrator_identities(source)?
    } else {
        BTreeSet::new()
    };
    let mut context = discovery_context(source, &runs, now);
    if include_managed {
        context.state_root = None;
        context.worktrees.clear();
        context.dispatched_sessions.clear();
    }
    let cutoff = if days == 0 {
        0
    } else {
        now.saturating_sub(i64::from(days) * 86_400)
    };
    let artifacts = FsArtifactSource::new(source.max_artifact_bytes);
    let mut records = Vec::new();
    for engine in Engine::ALL {
        let profiles = source.provider_profiles(engine)?;
        for profile in profiles {
            if !safe_absolute(&profile.path) {
                continue;
            }
            if !report(
                &records,
                &format!("Scanning {engine} account {}", profile.account),
            ) {
                anyhow::bail!("session discovery cancelled");
            }
            let result = match engine {
                Engine::Claude => claude::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
                Engine::Codex => codex::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
                Engine::Opencode => opencode::discover_sqlite(
                    &opencode::database_path(&profile.path, &source.home),
                    &profile.account,
                    &context,
                    cutoff,
                ),
                Engine::Kimi => kimi::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
                Engine::Grok => grok::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
            };
            if let Ok(mut rows) = result {
                if include_managed {
                    for row in &mut rows {
                        row.worker = runs.iter().any(|run| {
                            run.engine == row.engine
                                && (run.session.as_deref() == Some(&row.id)
                                    || run
                                        .session_history
                                        .iter()
                                        .any(|entry| entry.session == row.id))
                        });
                        row.orchestrator = runs.iter().any(|run| {
                            run.engine == row.engine && run.orch_session.as_deref() == Some(&row.id)
                        }) || orchestrators.contains(&(
                            row.engine,
                            profile
                                .path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or_default()
                                .to_owned(),
                            row.id.clone(),
                        ));
                    }
                }
                records.extend(rows);
            }
        }
    }
    let records = flatten_children(records);
    Ok(records
        .into_iter()
        .filter(|record| record.last_active.unwrap_or_default() >= cutoff)
        .collect::<Vec<_>>())
}

fn read_orchestrator_identities(
    source: &FilesystemPortalSource,
) -> Result<BTreeSet<(Engine, String, String)>> {
    let entries = match std::fs::read_dir(&source.paths.orchestrators) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => return Err(error.into()),
    };
    let mut identities = BTreeSet::new();
    for entry in entries.flatten() {
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if let Ok(Some(record)) =
            state::load::<neomax_core::orchestration::registry::OrchestratorRecord>(&entry.path())
        {
            identities.insert((record.engine, record.account_dir, record.session));
        }
    }
    Ok(identities)
}

fn flatten_children(records: impl IntoIterator<Item = SessionRecord>) -> Vec<SessionRecord> {
    let mut output = Vec::new();
    for record in records {
        flatten_record(record, &mut output);
    }
    output
}

fn flatten_record(mut record: SessionRecord, output: &mut Vec<SessionRecord>) {
    let children = std::mem::take(&mut record.children);
    output.push(record);
    for mut child in children {
        child.kind = SessionKind::NativeSubagent;
        flatten_record(child, output);
    }
}

fn discovery_context(
    source: &FilesystemPortalSource,
    runs: &[neomax_core::runs::RunRecord],
    now: i64,
) -> DiscoveryContext {
    let worktrees = runs
        .iter()
        .filter(|run| matches!(run.status, RunStatus::Running | RunStatus::Orphaned))
        .filter_map(|run| run.worktree.clone())
        .filter(|path| safe_absolute(path))
        .collect::<Vec<_>>();
    let dispatched_sessions = runs
        .iter()
        .flat_map(|run| {
            run.session
                .iter()
                .chain(run.orch_session.iter())
                .chain(run.session_history.iter().map(|entry| &entry.session))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let projects = state::load::<BTreeMap<String, Project>>(&source.paths.projects)
        .ok()
        .flatten()
        .unwrap_or_default();
    DiscoveryContext {
        now,
        active_window: SESSION_ACTIVITY_WINDOW_SECONDS,
        state_root: safe_absolute(&source.paths.state).then(|| source.paths.state.clone()),
        worktrees,
        dispatched_sessions,
        internal_sessions: BTreeSet::new(),
        orchestrator_sessions: BTreeSet::new(),
        project_resolver: Some(std::sync::Arc::new(move |path: &Path| {
            project_for_path(&projects, path)
        })),
    }
}

fn project_for_path(projects: &BTreeMap<String, Project>, path: &Path) -> Option<String> {
    if !safe_absolute(path) {
        return None;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    projects
        .iter()
        .filter(|(_, project)| {
            if !safe_absolute(&project.root) {
                return false;
            }
            let root = project
                .root
                .canonicalize()
                .unwrap_or_else(|_| project.root.clone());
            canonical == root || canonical.starts_with(root)
        })
        .max_by_key(|(_, project)| project.root.components().count())
        .map(|(name, _)| name.clone())
}

fn safe_absolute(path: &Path) -> bool {
    path.is_absolute() && !is_rooted_but_not_absolute(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::source::FilesystemPortalSource;

    #[test]
    fn ownership_registry_scan_is_read_only_and_keeps_provider_profile_identity() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        std::fs::create_dir_all(&source.paths.orchestrators).unwrap();
        let path = source.paths.orchestrators.join("old.json");
        let bytes = serde_json::to_vec(&serde_json::json!({"session":"same-id","engine":"codex","account_dir":".codex2","started":1,"last_seen":1})).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let identities = read_orchestrator_identities(&source).unwrap();
        assert!(identities.contains(&(Engine::Codex, ".codex2".into(), "same-id".into())));
        assert!(!identities.contains(&(Engine::Codex, ".codex".into(), "same-id".into())));
        assert!(!identities.contains(&(Engine::Claude, ".codex2".into(), "same-id".into())));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }

    #[test]
    fn empty_relocated_installation_has_no_discovered_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let sessions = discover_sessions(&source, 3, 1_800_000_000).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn project_resolver_prefers_the_most_specific_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let nested = root.join("nested");
        let source = nested.join("src");
        std::fs::create_dir_all(&source).unwrap();
        let projects = BTreeMap::from([
            ("root".into(), Project::portable(root, "root".into(), 1)),
            ("nested".into(), Project::portable(nested, "nest".into(), 1)),
        ]);
        assert_eq!(project_for_path(&projects, &source), Some("nested".into()));
    }

    #[test]
    fn nested_native_subagents_are_flattened_without_losing_parent_links() {
        let mut main = SessionRecord::with_identity("main", Engine::Claude, "1");
        let mut child = SessionRecord::with_identity("child", Engine::Claude, "1");
        child.kind = SessionKind::NativeSubagent;
        child.parent_id = Some("main".into());
        let mut grandchild = SessionRecord::with_identity("grandchild", Engine::Claude, "1");
        grandchild.parent_id = Some("child".into());
        child.children.push(grandchild);
        main.children.push(child);

        let records = flatten_children([main]);
        assert_eq!(
            records
                .iter()
                .map(|record| record.id.as_str())
                .collect::<Vec<_>>(),
            ["main", "child", "grandchild"]
        );
        assert_eq!(records[1].parent_id.as_deref(), Some("main"));
        assert_eq!(records[2].parent_id.as_deref(), Some("child"));
        assert!(records[2].children.is_empty());
    }

    #[test]
    fn project_resolution_ignores_relative_roots() {
        let projects = BTreeMap::from([(
            "relative".into(),
            Project::portable(PathBuf::from("relative/project"), "relative".into(), 1),
        )]);
        assert_eq!(
            project_for_path(&projects, Path::new("relative/project/src")),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn session_discovery_ignores_partial_windows_roots() {
        assert!(!safe_absolute(Path::new(r"\state")));
        assert!(!safe_absolute(Path::new(r"C:state")));
        assert!(safe_absolute(Path::new(r"C:\state")));
    }
}
