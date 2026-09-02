use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::Result;
use crate::git::inspection::{GitCommandOutput, GitCommandRunner};
use crate::git::{
    ArtifactCleanupMode, GitWorktreeManager, ManagedArtifactCleaner, WorktreeCleanupPolicy,
    WorktreeOutcome, WorktreeTarget,
};

#[derive(Debug, Clone, Copy)]
struct HermeticGit;

impl GitCommandRunner for HermeticGit {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        let mut safe_args = safe_git_args();
        safe_args.extend(args.iter().cloned());
        let output = hermetic_command(cwd, &safe_args)
            .output()
            .map_err(crate::Error::Io)?;
        Ok(GitCommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let arguments: Vec<String> = args.iter().map(|argument| (*argument).to_owned()).collect();
    let output = HermeticGit.run(cwd, &arguments).unwrap();
    assert!(output.success, "{}", output.stderr);
}

fn git_stdout(cwd: &Path, args: &[&str]) -> String {
    let arguments: Vec<String> = args.iter().map(|argument| (*argument).to_owned()).collect();
    let output = HermeticGit.run(cwd, &arguments).unwrap();
    assert!(output.success, "{}", output.stderr);
    output.stdout.trim().to_owned()
}

fn repository(root: &Path) -> (PathBuf, PathBuf, String) {
    let repo = root.join("repo");
    let worktree = root.join("worktrees").join("task");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "Test User"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(&repo, &["config", "commit.gpgSign", "true"]);
    git(&repo, &["config", "tag.gpgSign", "true"]);
    #[cfg(unix)]
    {
        let hook = repo.join(".git/hooks/pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\nprintf hooked > \"$GIT_DIR/neomax-hook-fired\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(repo.join("file.txt"), "base\n").unwrap();
    git(&repo, &["add", "file.txt"]);
    git(&repo, &["commit", "-qm", "base"]);
    assert!(
        !repo.join(".git/neomax-hook-fired").exists(),
        "fixture hook executed despite the fail-closed hook path"
    );
    let branch = git_stdout(&repo, &["branch", "--show-current"]);
    git(&repo, &["branch", "task"]);
    fs::create_dir_all(worktree.parent().unwrap()).unwrap();
    git(
        &repo,
        &["worktree", "add", "-q", worktree.to_str().unwrap(), "task"],
    );
    (repo, worktree, branch)
}

fn target(repo: &Path, worktree: &Path, base: &str) -> WorktreeTarget {
    WorktreeTarget::new(repo, worktree, "task", base)
}

#[test]
fn preserves_and_reports_a_changed_worktree() -> crate::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join("file.txt"), "changed\n").unwrap();
    fs::write(worktree.join("new.txt"), "new\n").unwrap();
    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target(&repo, &worktree, &base),
            WorktreeCleanupPolicy::remove_unchanged(),
            &HermeticGit,
        )
        .unwrap();
    let WorktreeOutcome::HasChanges { inspection } = outcome else {
        return Err(crate::Error::Message("expected changed worktree".into()));
    };
    assert_eq!(
        inspection.files_touched,
        BTreeSet::from(["file.txt".into(), "new.txt".into()])
    );
    assert!(worktree.exists());
    Ok(())
}

#[test]
fn preserves_and_reports_committed_work() -> crate::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join("committed.txt"), "work\n").unwrap();
    git(&worktree, &["add", "committed.txt"]);
    git(&worktree, &["commit", "-qm", "work"]);
    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target(&repo, &worktree, &base),
            WorktreeCleanupPolicy::remove_unchanged(),
            &HermeticGit,
        )
        .unwrap();
    let WorktreeOutcome::HasChanges { inspection } = outcome else {
        return Err(crate::Error::Message("expected changed worktree".into()));
    };
    assert_eq!(
        inspection.files_touched,
        BTreeSet::from(["committed.txt".into()])
    );
    assert!(worktree.exists());
    Ok(())
}

#[test]
fn cleans_only_when_the_caller_allows_unchanged_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target(&repo, &worktree, &base),
            WorktreeCleanupPolicy::remove_unchanged(),
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(outcome, WorktreeOutcome::Cleaned);
    assert!(!worktree.exists());
    let output = HermeticGit
        .run(
            &repo,
            &[
                "show-ref".into(),
                "--verify".into(),
                "refs/heads/task".into(),
            ],
        )
        .unwrap();
    assert!(!output.success);
}

#[test]
fn preserves_ignored_non_generated_content_during_whole_cleanup() -> crate::Result<()> {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join(".gitignore"), ".env\n").unwrap();
    git(&worktree, &["add", ".gitignore"]);
    git(&worktree, &["commit", "-qm", "ignore local environment"]);
    fs::write(worktree.join(".env"), "TOKEN=preserve\n").unwrap();

    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target(&repo, &worktree, &base),
            WorktreeCleanupPolicy::remove_unchanged(),
            &HermeticGit,
        )
        .unwrap();

    let WorktreeOutcome::HasChanges { inspection } = outcome else {
        return Err(crate::Error::Message(
            "expected ignored content to preserve the worktree".into(),
        ));
    };
    assert!(inspection.dirty);
    assert!(inspection.files_touched.contains(".env"));
    assert_eq!(fs::read_to_string(worktree.join(".env")).unwrap(), "TOKEN=preserve\n");
    Ok(())
}

#[test]
fn keeps_an_unchanged_worktree_when_cleanup_is_not_allowed() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target(&repo, &worktree, &base),
            WorktreeCleanupPolicy::keep(),
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(outcome, WorktreeOutcome::EmptyKept);
    assert!(worktree.exists());
}

#[test]
fn records_a_vanished_worktree_without_falling_back() {
    let temp = tempfile::tempdir().unwrap();
    let target = target(temp.path(), &temp.path().join("missing"), "main");
    let outcome = GitWorktreeManager
        .inspect_and_cleanup_with_runner(
            &target,
            WorktreeCleanupPolicy::remove_unchanged(),
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(outcome, WorktreeOutcome::Vanished);
}

#[test]
fn artifact_cleanup_reports_dry_run_and_removes_only_ignored_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(
        worktree.join(".gitignore"),
        "node_modules/\ntarget/\ndist/\nscratch/\n.env\n",
    )
    .unwrap();
    fs::create_dir_all(worktree.join("node_modules/.bin")).unwrap();
    fs::write(worktree.join("node_modules/.bin/tool"), "generated").unwrap();
    fs::create_dir_all(worktree.join("target/debug")).unwrap();
    fs::write(worktree.join("target/debug/app"), "binary").unwrap();
    fs::create_dir_all(worktree.join("dist/assets")).unwrap();
    fs::write(worktree.join("dist/assets/app.js"), "bundle").unwrap();
    fs::create_dir_all(worktree.join("scratch")).unwrap();
    fs::write(worktree.join("scratch/notes.txt"), "keep").unwrap();
    fs::write(worktree.join(".env"), "TOKEN=keep\n").unwrap();
    fs::write(worktree.join("source.txt"), "source\n").unwrap();

    let cleaner = ManagedArtifactCleaner::new(temp.path().join("worktrees"));
    let dry_run = cleaner
        .cleanup_with_runner(
            &target(&repo, &worktree, &base),
            ArtifactCleanupMode::DryRun,
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(dry_run.found, 3);
    assert_eq!(dry_run.eligible, 3);
    assert_eq!(dry_run.removed, 0);
    assert_eq!(dry_run.skipped, 0);
    assert!(dry_run.bytes_reclaimable > 0);
    assert!(worktree.join("node_modules").exists());
    assert!(worktree.join("target").exists());
    assert!(worktree.join("dist").exists());

    let applied = cleaner
        .cleanup_with_runner(
            &target(&repo, &worktree, &base),
            ArtifactCleanupMode::Apply,
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(applied.found, 3);
    assert_eq!(applied.eligible, 3);
    assert_eq!(applied.removed, 3);
    assert_eq!(applied.skipped, 0);
    assert_eq!(applied.bytes_reclaimed, dry_run.bytes_reclaimable);
    assert!(!worktree.join("node_modules").exists());
    assert!(!worktree.join("target").exists());
    assert!(!worktree.join("dist").exists());
    assert_eq!(fs::read_to_string(worktree.join("scratch/notes.txt")).unwrap(), "keep");
    assert_eq!(fs::read_to_string(worktree.join(".env")).unwrap(), "TOKEN=keep\n");
    assert_eq!(fs::read_to_string(worktree.join("source.txt")).unwrap(), "source\n");
}

#[test]
fn artifact_cleanup_skips_generated_directories_with_tracked_files() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join(".gitignore"), "build/\n").unwrap();
    fs::create_dir_all(worktree.join("build")).unwrap();
    fs::write(worktree.join("build/tracked.txt"), "source\n").unwrap();
    git(&worktree, &["add", "-f", "build/tracked.txt"]);
    git(&worktree, &["commit", "-qm", "tracked build output"]);

    let cleaner = ManagedArtifactCleaner::new(temp.path().join("worktrees"));
    let report = cleaner
        .cleanup_with_runner(
            &target(&repo, &worktree, &base),
            ArtifactCleanupMode::Apply,
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(report.found, 1);
    assert_eq!(report.eligible, 0);
    assert_eq!(report.removed, 0);
    assert_eq!(report.skipped, 1);
    assert!(worktree.join("build/tracked.txt").exists());
}

#[test]
fn artifact_cleanup_rejects_worktrees_outside_the_managed_root() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join(".gitignore"), "dist/\n").unwrap();
    fs::create_dir_all(worktree.join("dist")).unwrap();
    fs::write(worktree.join("dist/app.js"), "bundle").unwrap();

    let cleaner = ManagedArtifactCleaner::new(temp.path().join("other-worktrees"));
    let result = cleaner.cleanup_with_runner(
        &target(&repo, &worktree, &base),
        ArtifactCleanupMode::Apply,
        &HermeticGit,
    );
    assert!(matches!(result, Err(crate::Error::Conflict(_))));
    assert!(worktree.join("dist/app.js").exists());
}

#[cfg(unix)]
#[test]
fn artifact_cleanup_does_not_follow_symlink_candidates() {
    let temp = tempfile::tempdir().unwrap();
    let (repo, worktree, base) = repository(temp.path());
    fs::write(worktree.join(".gitignore"), "dist/\n").unwrap();
    fs::create_dir_all(worktree.join("real-output")).unwrap();
    fs::write(worktree.join("real-output/app.js"), "bundle").unwrap();
    std::os::unix::fs::symlink(
        worktree.join("real-output"),
        worktree.join("dist"),
    )
    .unwrap();

    let cleaner = ManagedArtifactCleaner::new(temp.path().join("worktrees"));
    let report = cleaner
        .cleanup_with_runner(
            &target(&repo, &worktree, &base),
            ArtifactCleanupMode::Apply,
            &HermeticGit,
        )
        .unwrap();
    assert_eq!(report.found, 1);
    assert_eq!(report.eligible, 0);
    assert_eq!(report.removed, 0);
    assert_eq!(report.skipped, 1);
    assert!(worktree.join("dist").exists());
    assert!(worktree.join("real-output/app.js").exists());
}

fn safe_git_args() -> Vec<String> {
    [
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
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn hermetic_command(cwd: &Path, args: &[String]) -> Command {
    let fixture_root = cwd.join(".neomax-git-fixture");
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("HOME", &fixture_root)
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
        .env("PATH", fixture_path());
    command
}

fn fixture_path() -> OsString {
    #[cfg(windows)]
    {
        std::env::var_os("PATH").unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        std::env::join_paths([Path::new("/usr/bin"), Path::new("/bin")])
            .expect("fixture PATH entries are valid")
    }
}
