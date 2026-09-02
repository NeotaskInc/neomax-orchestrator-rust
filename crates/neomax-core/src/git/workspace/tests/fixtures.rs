use std::fs;
use std::path::{Path, PathBuf};

use crate::git::invoke;

pub fn git(cwd: &Path, args: &[&str]) {
    let mut safe_args = vec![
        "-c",
        "core.hooksPath=__neomax_no_test_hooks__",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
    ];
    safe_args.extend_from_slice(args);
    let result = invoke(cwd, safe_args).unwrap();
    assert!(result.success, "{}", result.stderr_text());
}

pub fn repository(root: &Path) -> (PathBuf, String) {
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "Test User"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    fs::write(repo.join("base.txt"), "base\n").unwrap();
    git(&repo, &["add", "base.txt"]);
    git(&repo, &["commit", "-qm", "base"]);
    (repo, "main".into())
}

pub fn integration_request(repo: &Path, plan: &str) -> super::super::IntegrationRequest {
    super::super::IntegrationRequest::new(repo, plan, Some("main".into()), None)
}
