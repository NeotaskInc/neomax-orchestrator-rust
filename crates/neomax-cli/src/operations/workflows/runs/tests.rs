use super::cleanup::{ArtifactTotals, automatic_tidy_allows};
use super::shared::{format_timestamp, run_match, searchable_fields};
use neomax_core::Engine;
use neomax_core::git::ArtifactCleanupReport;
use neomax_core::runs::{RunRecord, RunStatus};
use serde_json::json;

#[test]
fn searchable_fields_include_prompt_files_branch_and_repo() {
    let run = RunRecord::new(
        "run-1",
        neomax_core::Engine::Codex,
        "model",
        "fix parser",
        "/account",
        "/worktree",
        1,
    );
    assert!(
        searchable_fields(&run)
            .iter()
            .any(|value| value == "fix parser")
    );
    assert!(run_match(&run).id == "run-1");
}

#[test]
fn timestamp_format_is_deterministic() {
    assert_eq!(format_timestamp(0), "01-01 00:00:00");
}

#[test]
fn artifact_totals_are_reported_without_path_details() {
    let report = ArtifactCleanupReport {
        found: 3,
        eligible: 2,
        removed: 2,
        skipped: 1,
        bytes_reclaimable: 120,
        bytes_reclaimed: 100,
    };
    let mut totals = ArtifactTotals::default();
    totals.add(&report);

    assert_eq!(
        totals.json(),
        json!({
            "found": 3,
            "eligible": 2,
            "removed": 2,
            "skipped": 1,
            "bytes_reclaimable": 120,
            "bytes_reclaimed": 100,
        })
    );
}

#[test]
fn automatic_tidy_excludes_killed_and_resumable_runs() {
    let mut run = RunRecord::new(
        "run",
        Engine::Codex,
        "model",
        "task",
        "profile",
        "worktree",
        1,
    );
    assert!(automatic_tidy_allows(&run, RunStatus::Done));
    assert!(automatic_tidy_allows(&run, RunStatus::Error));
    assert!(automatic_tidy_allows(&run, RunStatus::Integrated));
    assert!(!automatic_tidy_allows(&run, RunStatus::Limit));
    assert!(!automatic_tidy_allows(&run, RunStatus::Interrupted));

    run.killed = true;
    assert!(!automatic_tidy_allows(&run, RunStatus::Done));
}
