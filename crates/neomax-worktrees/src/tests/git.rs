use std::fs;
use std::time::Duration;

#[cfg(unix)]
use std::time::Instant;

use tempfile::tempdir;

use super::fixtures::{repository, run};
use crate::git::{ConfiguredGit, GitProcessConfig, GitRunner, ProcessGit, args};

#[test]
fn configured_runner_rejects_excessive_git_output() {
    let temp = tempdir().unwrap();
    let repo = repository(temp.path(), "service-a");
    fs::write(repo.join("message.txt"), "output-line\n".repeat(64)).unwrap();
    run(
        &ProcessGit,
        &repo,
        &["commit", "--allow-empty", "-F", "message.txt"],
    );
    let runner = ConfiguredGit::new(GitProcessConfig::new(Duration::from_secs(2), 128));
    let error = runner
        .run(&repo, &args(["log", "--format=%B", "-1"]))
        .unwrap_err();
    assert!(error.to_string().contains("output exceeded 128 bytes"));
}

#[cfg(unix)]
#[test]
fn configured_runner_terminates_and_reaps_a_timed_out_git_process() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let repo = repository(temp.path(), "service-a");
    let hook = repo.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nsleep 2\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(repo.join("change.txt"), "change\n").unwrap();
    super::fixtures::run(&ProcessGit, &repo, &["add", "change.txt"]);
    let runner = ConfiguredGit::new(GitProcessConfig::new(Duration::from_millis(100), 4096));
    let started = Instant::now();
    let mut commit_args = args([
        "-c",
        "core.hooksPath=.git/hooks",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
    ]);
    commit_args.extend(args(["commit", "-m", "timeout"]));
    let error = runner.run(&repo, &commit_args).unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(1));
}
