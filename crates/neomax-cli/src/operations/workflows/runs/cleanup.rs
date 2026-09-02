use std::collections::BTreeMap;
use std::fs;

use anyhow::Result;
use neomax_core::git::{
    ArtifactCleanupMode, ArtifactCleanupReport, GitWorktreeManager, WorktreeCleanupPolicy,
    WorktreeOutcome,
};
use neomax_core::runs::{RunStatus, RunStore, SystemProcessProbe, effective_status, worker_alive};
use serde_json::json;

use super::super::args;
use super::shared::{append_event, history_store, owned_by_other};
use super::worktrees::{
    artifact_report_json, cleanup_artifacts, force_cleanup, merged_and_clean, worktree_target,
};
use crate::context::RuntimeContext;
use crate::output;

#[derive(Debug, Default)]
pub(super) struct ArtifactTotals {
    found: u64,
    eligible: u64,
    removed: u64,
    skipped: u64,
    bytes_reclaimable: u64,
    bytes_reclaimed: u64,
}

impl ArtifactTotals {
    pub(super) fn add(&mut self, report: &ArtifactCleanupReport) {
        self.found = self.found.saturating_add(report.found);
        self.eligible = self.eligible.saturating_add(report.eligible);
        self.removed = self.removed.saturating_add(report.removed);
        self.skipped = self.skipped.saturating_add(report.skipped);
        self.bytes_reclaimable = self
            .bytes_reclaimable
            .saturating_add(report.bytes_reclaimable);
        self.bytes_reclaimed = self.bytes_reclaimed.saturating_add(report.bytes_reclaimed);
    }

    pub(super) fn json(&self) -> serde_json::Value {
        json!({
            "found": self.found,
            "eligible": self.eligible,
            "removed": self.removed,
            "skipped": self.skipped,
            "bytes_reclaimable": self.bytes_reclaimable,
            "bytes_reclaimed": self.bytes_reclaimed,
        })
    }
}

pub(crate) fn clean(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, &[], &["--done", "--force", "--any", "--json"])?;
    let store = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let targets = if parsed.has("--done") {
        store
            .all()?
            .into_iter()
            .filter(|run| effective_status(run, &probe).is_terminal())
            .collect::<Vec<_>>()
    } else {
        let id = parsed.positional(0, "clean")?;
        vec![store.load(id)?]
    };
    let force = parsed.has("--force");
    let mut cleaned = Vec::new();
    let mut skipped = Vec::new();
    let mut artifacts = Vec::new();
    let mut artifact_totals = ArtifactTotals::default();
    for run in targets {
        if !parsed.has("--any") && owned_by_other(context, &run)? {
            skipped
                .push(json!({"id": run.id, "reason": "another live orchestrator owns this run"}));
            continue;
        }
        if worker_alive(&run, &probe) {
            skipped.push(json!({"id": run.id, "reason": "worker is still running"}));
            continue;
        }
        let status = effective_status(&run, &probe);
        if !status.is_terminal() {
            skipped.push(json!({"id": run.id, "reason": "run is not terminal"}));
            continue;
        }
        if run.killed && run.status == RunStatus::Aborted && !force {
            skipped.push(json!({"id": run.id, "reason": "killed run is resumeable; use --force"}));
            continue;
        }
        if !force && !run.is_acknowledged() {
            skipped.push(json!({"id": run.id, "reason": "run has not been acknowledged"}));
            continue;
        }
        let target = match worktree_target(&run) {
            Ok(target) => target,
            Err(error) => {
                skipped.push(json!({
                    "id": run.id,
                    "reason": format!("invalid worktree record: {error}"),
                }));
                continue;
            }
        };
        if let Some(target) = target {
            if !force {
                let report = match cleanup_artifacts(context, &target, ArtifactCleanupMode::Apply) {
                    Ok(report) => report,
                    Err(error) => {
                        skipped.push(json!({
                            "id": run.id,
                            "reason": format!("artifact cleanup failed closed: {error}"),
                        }));
                        continue;
                    }
                };
                artifact_totals.add(&report);
                artifacts.push(json!({
                    "id": run.id,
                    "report": artifact_report_json(&report),
                }));
            }
            let outcome = if force {
                force_cleanup(context, &target)?
            } else {
                GitWorktreeManager
                    .inspect_and_cleanup(&target, WorktreeCleanupPolicy::remove_unchanged())?
            };
            match outcome {
                WorktreeOutcome::Vanished
                | WorktreeOutcome::Cleaned
                | WorktreeOutcome::EmptyKept => {}
                WorktreeOutcome::HasChanges { inspection } => {
                    skipped.push(json!({
                        "id": run.id,
                        "reason": "worktree has unmerged work",
                        "dirty": inspection.dirty,
                        "commits_ahead": inspection.commits_ahead,
                    }));
                    continue;
                }
            }
        }
        let mut archived = run.clone();
        archived.status = status;
        archived.acknowledged = Some(true);
        append_event(context, &archived, "cleaned", BTreeMap::new())?;
        history_store(context).archive_or_spill(&archived, None, context.now)?;
        fs::remove_file(store.path(&run.id))?;
        cleaned.push(run.id);
    }
    let report = json!({
        "cleaned": cleaned,
        "skipped": skipped,
        "force": force,
        "artifacts": artifacts,
        "artifact_totals": artifact_totals.json(),
    });
    if parsed.has("--json") {
        return output::json(&report);
    }
    for id in report["cleaned"].as_array().into_iter().flatten() {
        println!("cleaned {}", id.as_str().unwrap_or_default());
    }
    if let Some(skipped) = report["skipped"].as_array() {
        if !skipped.is_empty() {
            println!("clean: skipped {} run(s)", skipped.len());
            for item in skipped {
                println!(
                    "  {}: {}",
                    item["id"].as_str().unwrap_or("?"),
                    item["reason"].as_str().unwrap_or("unknown reason")
                );
            }
        }
    }
    if report["artifact_totals"]["found"]
        .as_u64()
        .unwrap_or_default()
        > 0
    {
        println!(
            "clean: {} artifact(s) removed, {} byte(s) reclaimed",
            report["artifact_totals"]["removed"]
                .as_u64()
                .unwrap_or_default(),
            report["artifact_totals"]["bytes_reclaimed"]
                .as_u64()
                .unwrap_or_default(),
        );
    }
    Ok(())
}

pub(crate) fn tidy(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(
        args,
        &["--project"],
        &["--any", "--automatic", "--dry-run", "--json"],
    )?;
    let project = parsed.value("--project");
    let any_owner = parsed.has("--any");
    let automatic = parsed.has("--automatic");
    let dry_run = parsed.has("--dry-run");
    let store = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let mut eligible = Vec::new();
    let mut skipped = Vec::new();
    let mut artifacts = Vec::new();
    let mut artifact_totals = ArtifactTotals::default();
    for run in store.all()? {
        if project.is_some_and(|expected| run.project.as_deref() != Some(expected)) {
            continue;
        }
        let status = effective_status(&run, &probe);
        if !status.is_terminal() {
            continue;
        }
        if automatic && !automatic_tidy_allows(&run, status) {
            skipped.push(json!({
                "id": run.id,
                "reason": "automatic tidy preserves resumable or killed runs",
            }));
            continue;
        }
        if worker_alive(&run, &probe) {
            skipped.push(json!({"id": run.id, "reason": "worker is still running"}));
            continue;
        }
        if !any_owner && owned_by_other(context, &run)? {
            skipped.push(json!({
                "id": run.id,
                "reason": "another live orchestrator owns this run"
            }));
            continue;
        }
        let target = match worktree_target(&run) {
            Ok(target) => target,
            Err(error) => {
                skipped.push(json!({
                    "id": run.id,
                    "reason": format!("invalid worktree record: {error}"),
                }));
                continue;
            }
        };
        if let Some(target) = target {
            let mode = if dry_run {
                ArtifactCleanupMode::DryRun
            } else {
                ArtifactCleanupMode::Apply
            };
            let artifact_report = match cleanup_artifacts(context, &target, mode) {
                Ok(report) => report,
                Err(error) => {
                    skipped.push(json!({
                        "id": run.id,
                        "reason": format!("artifact cleanup failed closed: {error}"),
                    }));
                    continue;
                }
            };
            artifact_totals.add(&artifact_report);
            artifacts.push(json!({
                "id": run.id,
                "report": artifact_report_json(&artifact_report),
            }));
        }
        match merged_and_clean(&run) {
            Ok(true) => eligible.push(run),
            Ok(false) => skipped.push(json!({
                "id": run.id,
                "reason": "merge and clean state were not confirmed"
            })),
            Err(error) => skipped.push(json!({
                "id": run.id,
                "reason": format!("could not verify merge state: {error}")
            })),
        }
    }

    let mut cleaned = Vec::new();
    if !dry_run {
        for run in eligible {
            let status = effective_status(&run, &probe);
            if !status.is_terminal() {
                skipped.push(json!({
                    "id": run.id,
                    "reason": "run stopped being terminal during tidy"
                }));
                continue;
            }
            let target = match worktree_target(&run) {
                Ok(target) => target,
                Err(error) => {
                    skipped.push(json!({
                        "id": run.id,
                        "reason": format!("invalid worktree record during tidy: {error}"),
                    }));
                    continue;
                }
            };
            if let Some(target) = target {
                match GitWorktreeManager
                    .inspect_and_cleanup(&target, WorktreeCleanupPolicy::remove_unchanged())?
                {
                    WorktreeOutcome::Vanished | WorktreeOutcome::Cleaned => {}
                    WorktreeOutcome::EmptyKept => {
                        skipped.push(json!({
                            "id": run.id,
                            "reason": "worktree was not removed"
                        }));
                        continue;
                    }
                    WorktreeOutcome::HasChanges { .. } => {
                        skipped.push(json!({
                            "id": run.id,
                            "reason": "worktree changed after merge verification"
                        }));
                        continue;
                    }
                }
            }
            let mut archived = run.clone();
            archived.status = status;
            archived.acknowledged = Some(true);
            append_event(context, &archived, "tidied", BTreeMap::new())?;
            history_store(context).archive_or_spill(&archived, None, context.now)?;
            fs::remove_file(store.path(&run.id))?;
            cleaned.push(run.id);
        }
    } else {
        cleaned = eligible.into_iter().map(|run| run.id).collect();
    }
    let report = json!({
        "eligible": cleaned,
        "skipped": skipped,
        "dry_run": dry_run,
        "automatic": automatic,
        "project": project,
        "artifacts": artifacts,
        "artifact_totals": artifact_totals.json(),
    });
    if parsed.has("--json") {
        return output::json(&report);
    }
    if dry_run {
        println!(
            "tidy: {} finished run(s) are confirmed merged and clean",
            report["eligible"]
                .as_array()
                .map_or(0, |values| values.len())
        );
    } else {
        println!(
            "tidy: resolved {} merged run(s)",
            report["eligible"]
                .as_array()
                .map_or(0, |values| values.len())
        );
    }
    let artifact_label = if dry_run { "would remove" } else { "removed" };
    let totals = &report["artifact_totals"];
    if totals["found"].as_u64().unwrap_or_default() > 0 {
        println!(
            "tidy: {} artifact(s) found, {} {} and {} byte(s) {}",
            totals["found"].as_u64().unwrap_or_default(),
            if dry_run {
                totals["eligible"].as_u64().unwrap_or_default()
            } else {
                totals["removed"].as_u64().unwrap_or_default()
            },
            artifact_label,
            if dry_run {
                totals["bytes_reclaimable"].as_u64().unwrap_or_default()
            } else {
                totals["bytes_reclaimed"].as_u64().unwrap_or_default()
            },
            if dry_run { "reclaimable" } else { "reclaimed" },
        );
    }
    if let Some(skipped) = report["skipped"].as_array() {
        if !skipped.is_empty() {
            println!("tidy: skipped {} run(s)", skipped.len());
        }
    }
    Ok(())
}

pub(super) fn automatic_tidy_allows(run: &neomax_core::runs::RunRecord, status: RunStatus) -> bool {
    !run.killed
        && matches!(
            status,
            RunStatus::Done | RunStatus::Error | RunStatus::Integrated
        )
}
