use serde::Serialize;

use crate::operations::{CreateReport, ListReport, RemoveReport};

#[derive(Debug, Serialize)]
struct JsonCreate<'a> {
    action: &'static str,
    task: &'a str,
    branch: &'a str,
    worktree_root: String,
    created: Vec<JsonWorktree<'a>>,
    reused: Vec<JsonWorktree<'a>>,
    skipped: Vec<JsonWorktree<'a>>,
}

#[derive(Debug, Serialize)]
struct JsonWorktree<'a> {
    repository: String,
    path: String,
    branch: &'a str,
    base: &'a str,
    action: &'static str,
}

#[derive(Debug, Serialize)]
struct JsonList {
    worktrees: Vec<JsonListEntry>,
}

#[derive(Debug, Serialize)]
struct JsonListEntry {
    task: String,
    repository: String,
    path: String,
    branch: String,
}

#[derive(Debug, Serialize)]
struct JsonRemove<'a> {
    action: &'static str,
    task: &'a str,
    removed: Vec<JsonWorktree<'a>>,
}

pub fn create_text(report: &CreateReport, dry_run: bool) -> String {
    let mut lines = vec![format!(
        "{} '{}' on '{}' across:",
        if dry_run { "would create" } else { "creating" },
        report.plan.task,
        report.plan.branch
    )];
    for spec in &report.plan.specs {
        let status = match spec.action {
            crate::plan::CreateAction::CreateBranch => "create",
            crate::plan::CreateAction::UseBranch => "use existing branch",
            crate::plan::CreateAction::ReuseWorktree => "already present",
            crate::plan::CreateAction::SkipNotGit => "skip (not a Git repository)",
        };
        lines.push(format!(
            "{} -> {} ({status})",
            spec.relative_repository.display(),
            spec.path.display()
        ));
    }
    lines.join("\n")
}

pub fn create_json(report: &CreateReport, dry_run: bool) -> String {
    let payload = JsonCreate {
        action: if dry_run { "would-create" } else { "created" },
        task: &report.plan.task,
        branch: &report.plan.branch,
        worktree_root: report.plan.set_path.to_string_lossy().into_owned(),
        created: report.created.iter().map(json_worktree).collect(),
        reused: report.reused.iter().map(json_worktree).collect(),
        skipped: report.skipped.iter().map(json_worktree).collect(),
    };
    serde_json::to_string_pretty(&payload).expect("worktree output is serializable")
}

fn json_worktree(spec: &crate::plan::WorktreeSpec) -> JsonWorktree<'_> {
    JsonWorktree {
        repository: spec.relative_repository.to_string_lossy().into_owned(),
        path: spec.path.to_string_lossy().into_owned(),
        branch: &spec.branch,
        base: &spec.base,
        action: spec.action.as_str(),
    }
}

pub fn list_text(report: &ListReport) -> String {
    if report.entries.is_empty() {
        return "no worktree sets".into();
    }
    let mut lines = Vec::new();
    let mut current = None;
    for entry in &report.entries {
        if current != Some(entry.task.as_str()) {
            lines.push(entry.task.clone());
            current = Some(entry.task.as_str());
        }
        lines.push(format!("    {:20} {}", entry.repository, entry.branch));
    }
    lines.join("\n")
}

pub fn list_json(report: &ListReport) -> String {
    let payload = JsonList {
        worktrees: report
            .entries
            .iter()
            .map(|entry| JsonListEntry {
                task: entry.task.clone(),
                repository: entry.repository.clone(),
                path: entry.path.to_string_lossy().into_owned(),
                branch: entry.branch.clone(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&payload).expect("worktree output is serializable")
}

pub fn remove_text(report: &RemoveReport, dry_run: bool) -> String {
    format!(
        "{} {}",
        if dry_run { "would remove" } else { "removed" },
        report.task
    )
}

pub fn remove_json(report: &RemoveReport, dry_run: bool) -> String {
    let payload = JsonRemove {
        action: if dry_run { "would-remove" } else { "removed" },
        task: &report.task,
        removed: report
            .removed
            .iter()
            .map(|spec| JsonWorktree {
                repository: spec.relative_repository.to_string_lossy().into_owned(),
                path: spec.path.to_string_lossy().into_owned(),
                branch: &spec.branch,
                base: &spec.base,
                action: "remove",
            })
            .collect(),
    };
    serde_json::to_string_pretty(&payload).expect("worktree output is serializable")
}
