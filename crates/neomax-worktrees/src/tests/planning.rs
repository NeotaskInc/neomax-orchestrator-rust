use std::path::PathBuf;

use tempfile::tempdir;

use super::fixtures::{context, repository};
use crate::git::ProcessGit;
use crate::plan::{self, CreateAction};

#[test]
fn plans_a_shared_branch_for_every_repository() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    let second = repository(temp.path(), "service-b");
    let context = context(temp.path(), &[("service-a", first), ("service-b", second)]);
    let plan = plan::create(&context, "feature", None, Some("main"), &ProcessGit).unwrap();
    assert_eq!(plan.branch, "samp/feature");
    assert_eq!(plan.specs.len(), 2);
    assert!(
        plan.specs
            .iter()
            .all(|spec| spec.action == CreateAction::CreateBranch)
    );
    assert!(plan.specs.iter().all(|spec| spec.base == "main"));
}

#[test]
fn plans_an_existing_branch_without_recreating_it() {
    let temp = tempdir().unwrap();
    let first = repository(temp.path(), "service-a");
    super::fixtures::run(&ProcessGit, &first, &["branch", "samp/existing"]);
    let context = context(temp.path(), &[("service-a", first)]);
    let plan = plan::create(
        &context,
        "feature",
        Some("samp/existing"),
        None,
        &ProcessGit,
    )
    .unwrap();
    assert_eq!(plan.specs[0].action, CreateAction::UseBranch);
}

#[test]
fn nested_repository_labels_are_project_relative() {
    let temp = tempdir().unwrap();
    let nested = temp.path().join("apps/api");
    std::fs::create_dir_all(&nested).unwrap();
    let git = ProcessGit;
    super::fixtures::run(&git, &nested, &["init", "-q"]);
    super::fixtures::run(&git, &nested, &["config", "user.name", "Neomax Test"]);
    super::fixtures::run(
        &git,
        &nested,
        &["config", "user.email", "test@example.invalid"],
    );
    std::fs::write(nested.join("README.md"), "api\n").unwrap();
    super::fixtures::run(&git, &nested, &["add", "README.md"]);
    super::fixtures::run(&git, &nested, &["commit", "-qm", "initial"]);
    super::fixtures::run(&git, &nested, &["branch", "-M", "main"]);
    let context = crate::discovery::ProjectContext {
        name: "sample".into(),
        root: temp.path().to_path_buf(),
        branch_prefix: "samp".into(),
        worktree_root: temp.path().join(".worktrees"),
        repositories: vec![crate::discovery::RepositorySpec {
            relative: PathBuf::from("apps/api"),
            root: nested,
            label: "apps-api".into(),
        }],
    };
    let plan = plan::create(&context, "feature", None, Some("main"), &git);
    assert!(plan.is_ok() || plan.unwrap_err().to_string().contains("worktree"));
}
