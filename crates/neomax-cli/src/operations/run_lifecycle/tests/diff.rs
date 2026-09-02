use std::ffi::OsString;
use std::process::Command;

#[cfg(not(windows))]
use std::path::Path;

use neomax_core::runs::{RunStatus, RunStore};

use super::fixture::{fixture, run};
use crate::operations::run_lifecycle::{RunLifecycleCommand, RunLifecycleReport, execute};

#[test]
fn diff_reports_numstat_for_a_recorded_branch_without_a_shell() {
    let fixture = fixture();
    let repo = fixture.temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "fixture@example.invalid"]);
    git(&repo, &["config", "user.name", "Fixture"]);
    git(&repo, &["config", "commit.gpgSign", "true"]);
    git(&repo, &["config", "tag.gpgSign", "true"]);
    #[cfg(unix)]
    {
        let hook = repo.join(".git/hooks/pre-commit");
        std::fs::write(
            &hook,
            "#!/bin/sh\nprintf hooked > \"$GIT_DIR/neomax-hook-fired\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    assert!(!repo.join(".git/neomax-hook-fired").exists());
    git(&repo, &["checkout", "-qb", "feature"]);
    std::fs::write(repo.join("README.md"), "base\nfeature\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "feature"]);
    let mut record = run("diff-me", RunStatus::Done, fixture.context.cwd.clone());
    record.repo = Some(repo);
    record.branch = Some("feature".into());
    record.base = Some("HEAD~1".into());
    RunStore::new(&fixture.context.paths.runs)
        .create(&record)
        .unwrap();
    let report = execute(
        RunLifecycleCommand::Diff,
        &fixture.context,
        &["diff-me".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::Diff(report) = report else {
        panic!("expected diff report")
    };
    assert_eq!(report.files.len(), 1);
    assert!(report.adds > 0);
}

#[test]
fn subagent_diff_uses_structured_child_records() {
    let fixture = fixture();
    let mut record = run("parent", RunStatus::Done, fixture.context.cwd.clone());
    record.children = vec![serde_json::json!({
        "id": "agent-1",
        "edits": 2,
        "files": [{"path":"src/lib.rs","adds":4,"dels":1,"patch":"@@"}]
    })];
    RunStore::new(&fixture.context.paths.runs)
        .create(&record)
        .unwrap();
    let report = execute(
        RunLifecycleCommand::SubagentDiff,
        &fixture.context,
        &["agent-1".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::SubagentDiff(report) = report else {
        panic!("expected subagent diff report")
    };
    assert_eq!(report.edits, 2);
    assert_eq!(report.adds, 4);
    assert!(report.files[0].patch.is_none());

    let report = execute(
        RunLifecycleCommand::SubagentDiff,
        &fixture.context,
        &["agent-1".into(), "--patch".into()],
        None,
        None,
    )
    .unwrap();
    let RunLifecycleReport::SubagentDiff(report) = report else {
        panic!("expected subagent diff report")
    };
    assert_eq!(report.files[0].patch.as_deref(), Some("@@"));
}

fn git(repo: &std::path::Path, args: &[&str]) {
    let mut safe_args = vec![
        "-c",
        "core.hooksPath=__neomax_no_test_hooks__",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "tag.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.sshCommand=",
    ];
    safe_args.extend_from_slice(args);
    let fixture_root = repo.join(".neomax-git-fixture");
    let status = Command::new(fixture_git_binary())
        .args(safe_args)
        .current_dir(repo)
        .env_clear()
        .env("HOME", fixture_root.join("home"))
        .env("XDG_CONFIG_HOME", fixture_root.join("config"))
        .env("GIT_CONFIG_GLOBAL", fixture_root.join("global"))
        .env("GIT_CONFIG_SYSTEM", fixture_root.join("system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", fixture_root.join("missing-askpass"))
        .env("SSH_ASKPASS", fixture_root.join("missing-ssh-askpass"))
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("PATH", fixture_path())
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} failed", args);
}

fn fixture_path() -> OsString {
    #[cfg(windows)]
    {
        std::env::join_paths(windows_command_paths()).expect("fixture PATH entries are valid")
    }
    #[cfg(not(windows))]
    {
        std::env::join_paths([Path::new("/usr/bin"), Path::new("/bin")])
            .expect("fixture PATH entries are valid")
    }
}

#[cfg(windows)]
fn fixture_git_binary() -> std::path::PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("git.exe"))
        .find(|candidate| candidate.is_file())
        .expect("host git executable is available for the fixture")
}

#[cfg(not(windows))]
fn fixture_git_binary() -> std::path::PathBuf {
    std::path::PathBuf::from("git")
}

#[cfg(windows)]
fn windows_command_paths() -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Some(command_shell) = std::env::var_os("ComSpec") {
        if let Some(parent) = std::path::PathBuf::from(command_shell).parent() {
            paths.push(parent.to_path_buf());
        }
    }
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        let system32 = std::path::PathBuf::from(system_root).join("System32");
        paths.push(system32.join("WindowsPowerShell/v1.0"));
        paths.push(system32);
    }
    paths.sort();
    paths.dedup();
    paths
}
