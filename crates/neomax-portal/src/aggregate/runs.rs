use std::collections::BTreeMap;
use std::path::Path;

use neomax_core::projects::Project;
use neomax_core::runs::{RunRecord, SystemProcessProbe, effective_status};

use crate::model::RunView;

pub(crate) fn run_views(
    records: &[RunRecord],
    projects: &BTreeMap<String, Project>,
    probe: &SystemProcessProbe,
) -> Vec<RunView> {
    records
        .iter()
        .map(|run| run_view(run, projects, probe))
        .collect()
}

fn run_view(
    run: &RunRecord,
    projects: &BTreeMap<String, Project>,
    probe: &SystemProcessProbe,
) -> RunView {
    let status = effective_status(run, probe);
    let project = run.project.clone().or_else(|| {
        run.cwd
            .as_deref()
            .or(run.repo.as_deref())
            .and_then(|path| project_for_path(projects, path))
    });
    RunView {
        id: run.id.clone(),
        engine: run.engine.as_str().into(),
        account: run.account(),
        acct_no: Some(run.account()),
        status: status.as_str().into(),
        prompt: truncate(&run.prompt, 160),
        branch: run.branch.clone(),
        session: run.session.clone(),
        repo: run.repo.as_deref().and_then(file_name),
        children: run.children.len(),
        child_list: run.children.iter().take(24).cloned().collect(),
        effort: run.effort.clone(),
        ultra: run.ultra,
        opus: run.opus,
        model: (!run.model.is_empty()).then(|| run.model.clone()),
        tag: run.tag.as_deref().map(|value| truncate(value, 120)),
        attempt: run.attempt,
        goal: run.goal.as_deref().map(|value| truncate(value, 300)),
        pr_url: run.pr_url.clone(),
        acknowledged: run.is_acknowledged(),
        worktree: run.worktree.clone(),
        worktree_state: run.worktree_state.clone(),
        project,
        files_touched: run.files_touched.clone(),
        started: run.started,
        ended: run.ended,
        orch_session: run.orch_session.clone(),
    }
}

fn project_for_path(projects: &BTreeMap<String, Project>, path: &Path) -> Option<String> {
    projects
        .iter()
        .filter(|(_, project)| path == project.root || path.starts_with(&project.root))
        .max_by_key(|(_, project)| project.root.components().count())
        .map(|(name, _)| name.clone())
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(Into::into)
}

fn truncate(value: &str, length: usize) -> String {
    value.chars().take(length).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::projects::Project;
    use neomax_core::runs::RunRecord;

    #[test]
    fn project_resolution_prefers_the_most_specific_root() {
        let mut workspace = Project::portable("/workspace".into(), "workspace".into(), 0);
        workspace.root = "/workspace".into();
        let mut service = Project::portable("/workspace/service".into(), "service".into(), 0);
        service.root = "/workspace/service".into();
        let projects =
            BTreeMap::from([("workspace".into(), workspace), ("service".into(), service)]);
        assert_eq!(
            project_for_path(&projects, Path::new("/workspace/service/src")),
            Some("service".into())
        );
    }

    #[test]
    fn run_view_preserves_effective_status_and_safe_prompt_limits() {
        let run: RunRecord = serde_json::from_value(serde_json::json!({
            "id": "run-1",
            "engine": "claude",
            "profile": "/home/acct",
            "prompt": "x"
        }))
        .unwrap();
        let mut run = run;
        run.prompt = "x".repeat(400);
        let views = run_views(&[run], &BTreeMap::new(), &SystemProcessProbe);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].prompt.chars().count(), 160);
    }
}
