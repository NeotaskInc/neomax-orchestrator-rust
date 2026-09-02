#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;

use neomax_core::Engine;
use neomax_core::runs::{RunRecord, RunStatus, RunStore};

use support::E2eHarness;

#[test]
fn automatic_tidy_reclaims_artifacts_then_removes_merged_worktree() {
    let harness = E2eHarness::new([]);
    let paths = harness.state_paths();
    let repository = harness.workspace().to_path_buf();
    let base = harness.git(&repository, &["branch", "--show-current"]);
    let branch = "neomax-cleanup-fixture";
    let worktree = paths.worktrees.join("cleanup-run");
    fs::create_dir_all(&paths.worktrees).unwrap();
    harness.git(&repository, &["branch", branch]);
    harness.git(
        &repository,
        &["worktree", "add", "-q", worktree.to_str().unwrap(), branch],
    );

    fs::write(worktree.join(".gitignore"), "node_modules/\n.env\n").unwrap();
    fs::write(worktree.join("source.txt"), "source work\n").unwrap();
    fs::write(worktree.join(".env"), "TOKEN=preserve\n").unwrap();
    fs::create_dir_all(worktree.join("node_modules/pkg")).unwrap();
    fs::write(worktree.join("node_modules/pkg/index.js"), "generated\n").unwrap();

    let mut run = RunRecord::new(
        "cleanup-run",
        Engine::Codex,
        "model",
        "cleanup fixture",
        harness.home.join("profile"),
        &worktree,
        1,
    );
    run.status = RunStatus::Done;
    run.ended = Some(2);
    run.repo = Some(repository.clone());
    run.worktree = Some(worktree.clone());
    run.branch = Some(branch.into());
    run.base = Some(base.clone());
    run.base_ref = Some(base.clone());
    RunStore::new(&paths.runs).create(&run).unwrap();

    let first = harness.run(["tidy", "--automatic", "--any", "--json"]);
    let first = first.json();
    assert_eq!(first["artifact_totals"]["removed"], 1);
    assert!(first["eligible"].as_array().unwrap().is_empty());
    assert!(!worktree.join("node_modules").exists());
    assert!(worktree.join("source.txt").exists());
    assert_eq!(
        fs::read_to_string(worktree.join(".env")).unwrap(),
        "TOKEN=preserve\n"
    );
    assert!(harness.run_path("cleanup-run").exists());

    harness.git(&worktree, &["add", ".gitignore", "source.txt"]);
    harness.git(&worktree, &["commit", "-qm", "fixture work"]);
    harness.git(&repository, &["merge", "--ff-only", branch]);

    let second = harness.run(["tidy", "--automatic", "--any", "--json"]);
    let second = second.json();
    assert!(second["eligible"].as_array().unwrap().is_empty());
    assert!(worktree.exists());
    assert!(harness.run_path("cleanup-run").exists());
    assert_eq!(
        fs::read_to_string(worktree.join(".env")).unwrap(),
        "TOKEN=preserve\n"
    );

    fs::remove_file(worktree.join(".env")).unwrap();
    let third = harness.run(["tidy", "--automatic", "--any", "--json"]);
    let third = third.json();
    assert_eq!(third["eligible"], serde_json::json!(["cleanup-run"]));
    assert!(!worktree.exists());
    assert!(!harness.run_path("cleanup-run").exists());
    assert!(harness.invocations().is_empty());
    harness.assert_hermetic_invocations();
}
