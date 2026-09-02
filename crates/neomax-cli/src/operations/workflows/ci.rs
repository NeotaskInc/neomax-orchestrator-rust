use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::atomic::write_bytes_atomic;
use neomax_core::issues::{CiSyncAction, NEOMAX_CI_WORKFLOW, ci_sync_action};
use serde::Serialize;
use serde_json::json;

use super::args;
use super::catalog::{LocalCatalog, project_name};
use crate::context::RuntimeContext;
use crate::output;

const VALUE_FLAGS: &[&str] = &["--project"];
const SWITCH_FLAGS: &[&str] = &["--apply", "--force", "--json"];
const MAX_WORKFLOW_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
struct SyncRow {
    repository: String,
    path: String,
    action: String,
}

pub(super) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, VALUE_FLAGS, SWITCH_FLAGS)?;
    let project = project_name(context, parsed.value("--project"))?;
    let catalog = LocalCatalog::from_context(context);
    let targets = catalog.targets_for(&project)?;
    let apply = parsed.has("--apply");
    let force = parsed.has("--force");
    let mut rows = Vec::with_capacity(targets.len());
    for target in targets {
        let workflow = target.path.join(".github/workflows/neomax-ci.yml");
        let existing = read_workflow(&workflow)?;
        let action = ci_sync_action(existing.as_deref(), force);
        let should_write = apply && matches!(&action, CiSyncAction::Create | CiSyncAction::Update);
        let action_name = action_name(&action, apply);
        if should_write {
            write_bytes_atomic(&workflow, NEOMAX_CI_WORKFLOW.as_bytes())?;
        }
        rows.push(SyncRow {
            repository: target.name,
            path: workflow.display().to_string(),
            action: action_name,
        });
    }
    if parsed.has("--json") {
        return output::json(&json!({"project": project, "apply": apply, "rows": rows}));
    }
    for row in &rows {
        println!("  {:<24} {}", row.repository, row.action);
    }
    if !apply {
        println!("ci-sync: dry run; re-run with --apply to write managed workflows");
    }
    Ok(())
}

fn read_workflow(path: &Path) -> Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > MAX_WORKFLOW_BYTES {
        bail!("refusing to inspect oversized workflow {}", path.display());
    }
    Ok(Some(fs::read_to_string(path)?))
}

fn action_name(action: &CiSyncAction, apply: bool) -> String {
    match action {
        CiSyncAction::Unchanged => "unchanged".into(),
        CiSyncAction::Create => {
            if apply {
                "created".into()
            } else {
                "would-create".into()
            }
        }
        CiSyncAction::Update => {
            if apply {
                "updated".into()
            } else {
                "would-update".into()
            }
        }
        CiSyncAction::SkipHandEdited => "skip (hand-edited; use --force)".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_names_preserve_dry_run_and_apply_semantics() {
        assert_eq!(action_name(&CiSyncAction::Create, false), "would-create");
        assert_eq!(action_name(&CiSyncAction::Create, true), "created");
        assert_eq!(
            action_name(&CiSyncAction::SkipHandEdited, true),
            "skip (hand-edited; use --force)"
        );
    }
}
