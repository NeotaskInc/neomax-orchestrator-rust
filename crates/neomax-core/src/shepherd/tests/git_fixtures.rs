use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use tempfile::{TempDir, tempdir};

use crate::shepherd::GitInspector;

pub(super) fn git(repo: &Path, args: &[&str]) -> String {
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
    let output = hermetic_git_command(repo)
        .args(safe_args)
        .output()
        .unwrap_or_else(|error| panic!("git {:?}: {error}", args));
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(super) fn repository() -> TempDir {
    let repo = tempdir().unwrap();
    git(repo.path(), &["init"]);
    git(repo.path(), &["config", "user.name", "Neomax Test"]);
    git(
        repo.path(),
        &["config", "user.email", "neomax-test@example.invalid"],
    );
    git(repo.path(), &["config", "commit.gpgSign", "true"]);
    git(repo.path(), &["config", "tag.gpgSign", "true"]);
    #[cfg(unix)]
    {
        let hook = repo.path().join(".git/hooks/pre-commit");
        std::fs::write(
            &hook,
            "#!/bin/sh\nprintf hooked > \"$GIT_DIR/neomax-hook-fired\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(repo.path(), &["checkout", "-b", "main"]);
    std::fs::write(repo.path().join("README"), "base\n").unwrap();
    git(repo.path(), &["add", "README"]);
    git(repo.path(), &["commit", "-m", "base"]);
    assert!(!repo.path().join(".git/neomax-hook-fired").exists());
    repo
}

pub(super) fn commit(repo: &Path, message: &str, content: &str) {
    std::fs::write(repo.join("work.txt"), content).unwrap();
    git(repo, &["add", "work.txt"]);
    git(repo, &["commit", "-m", message]);
}

pub(super) fn inspector() -> GitInspector {
    GitInspector::new()
}

fn hermetic_git_command(repo: &Path) -> Command {
    let fixture_root = repo.join(".neomax-git-fixture");
    let mut command = Command::new("git");
    command
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
