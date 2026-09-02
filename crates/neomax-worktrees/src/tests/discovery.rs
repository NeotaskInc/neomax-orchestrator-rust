use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use super::fixtures::repository;
use crate::config::RuntimeConfig;
use crate::discovery;
use crate::git::ProcessGit;

#[test]
fn resolves_registered_project_and_keeps_repository_paths_relative() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let state = temp.path().join("state");
    fs::create_dir_all(&state).unwrap();
    let projects = serde_json::json!({
        "sample": {
            "root": temp.path(),
            "repos": ["service-a", "service-b"],
            "branch_prefix": "samp"
        }
    });
    fs::write(
        state.join("projects.json"),
        serde_json::to_vec_pretty(&projects).unwrap(),
    )
    .unwrap();
    let config = RuntimeConfig {
        home: state.clone(),
        project_dir: None,
        repos: None,
        branch_prefix: None,
        worktree_root: None,
        dry_run: false,
        json: false,
    };
    let context = discovery::resolve(&config, &first, &ProcessGit).unwrap();
    assert_eq!(context.name, "sample");
    assert_eq!(context.branch_prefix, "samp");
    assert_eq!(context.repositories[0].relative, PathBuf::from("service-a"));
    assert_eq!(context.repositories[1].relative, PathBuf::from("service-b"));
    assert_eq!(context.repositories[0].root, first.canonicalize().unwrap());
    assert_eq!(context.repositories[1].root, second.canonicalize().unwrap());
    assert_eq!(
        context.worktree_root,
        state.join("coordinated-worktrees/sample")
    );
}

#[test]
fn explicit_project_directory_overrides_the_git_root() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("workspace");
    fs::create_dir_all(&root).unwrap();
    let repository = repository(&root, "service-a");
    let state = temp.path().join("state");
    let config = RuntimeConfig {
        home: state,
        project_dir: Some(root.clone()),
        repos: Some(vec![PathBuf::from("service-a")]),
        branch_prefix: Some("demo".into()),
        worktree_root: Some(temp.path().join("worktrees")),
        dry_run: true,
        json: false,
    };
    let context = discovery::resolve(&config, &repository, &ProcessGit).unwrap();
    assert_eq!(context.root, root.canonicalize().unwrap());
    assert_eq!(context.name, "workspace");
    assert_eq!(context.branch_prefix, "demo");
    assert!(context.repositories[0].root.is_dir());
}
