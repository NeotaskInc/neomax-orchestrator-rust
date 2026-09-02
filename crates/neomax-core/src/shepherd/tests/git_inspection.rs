use crate::shepherd::{
    evaluate_merge_readiness, GitCommandOutput, GitCommandRunner, GitInspectionRequest,
    GitInspector, MergePolicy, ShepherdDecision, StoppedReason,
};

use super::git_fixtures::{commit, git, inspector, repository};
use crate::{Error, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeGit {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    responses: Arc<Mutex<Vec<GitCommandOutput>>>,
}

impl FakeGit {
    fn new(responses: Vec<GitCommandOutput>) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses)),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl GitCommandRunner for FakeGit {
    fn run(&self, _cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        self.calls.lock().unwrap().push(args.to_vec());
        let response = self.responses.lock().unwrap().first().cloned();
        let Some(response) = response else {
            return Err(Error::Message("fake git response exhausted".into()));
        };
        self.responses.lock().unwrap().remove(0);
        Ok(response)
    }
}

fn git_output(success: bool, stdout: &str, stderr: &str) -> GitCommandOutput {
    GitCommandOutput {
        success,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

#[test]
fn derives_default_base_and_counts_commits_ahead() {
    let repo = repository();
    git(repo.path(), &["checkout", "-b", "feature"]);
    commit(repo.path(), "feature one", "one\n");
    commit(repo.path(), "feature two", "two\n");

    let result = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()).branch("feature"))
        .unwrap();
    assert_eq!(result.base, "main");
    assert_eq!(result.base_ref, "main");
    assert_eq!(result.ahead, 2);
    assert!(!result.branch_is_ancestor_of_base);
    assert_eq!(result.head_sha.len(), 40);
    assert_eq!(result.base_sha.len(), 40);
}

#[test]
fn detects_branch_already_merged_into_explicit_base() {
    let repo = repository();
    git(repo.path(), &["checkout", "-b", "feature"]);
    commit(repo.path(), "feature", "feature\n");
    git(repo.path(), &["checkout", "main"]);
    git(
        repo.path(),
        &["merge", "--no-ff", "feature", "-m", "merge feature"],
    );

    let result = inspector()
        .inspect(
            &GitInspectionRequest::new(repo.path())
                .branch("feature")
                .base("main"),
        )
        .unwrap();
    assert!(result.branch_is_ancestor_of_base);
    assert_eq!(result.ahead, 0);
    assert_eq!(result.base, "main");
}

#[test]
fn readiness_input_preserves_head_for_moved_sha_detection() {
    let repo = repository();
    git(repo.path(), &["checkout", "-b", "feature"]);
    commit(repo.path(), "feature one", "one\n");
    let before = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()).branch("feature"))
        .unwrap();
    commit(repo.path(), "feature two", "two\n");
    let after = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()).branch("feature"))
        .unwrap();
    let decision = evaluate_merge_readiness(
        &after.readiness_input(Some(before.head_sha.clone())),
        MergePolicy::default(),
    );
    assert!(matches!(
        decision,
        ShepherdDecision::Stopped {
            reason: StoppedReason::HeadMoved { expected, actual },
            ..
        } if expected == before.head_sha && actual == after.head_sha
    ));
}

#[test]
fn missing_branch_is_reported_without_becoming_zero_ahead() {
    let repo = repository();
    let error = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()).branch("missing"))
        .unwrap_err();
    assert!(
        matches!(error, crate::Error::NotFound(message) if message.contains("branch 'missing'"))
    );
}

#[test]
fn detached_head_without_an_explicit_branch_is_an_error() {
    let repo = repository();
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    git(repo.path(), &["checkout", "--detach", &head]);
    let error = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()))
        .unwrap_err();
    assert!(matches!(error, crate::Error::Message(message) if message.contains("detached HEAD")));
}

#[test]
fn non_repository_path_is_an_error() {
    let repo = tempfile::tempdir().unwrap();
    let error = inspector()
        .inspect(&GitInspectionRequest::new(repo.path()).branch("main"))
        .unwrap_err();
    assert!(
        matches!(error, crate::Error::Message(message) if message.contains("not a Git repository"))
    );
}

#[test]
fn origin_fetch_is_injected_and_keeps_the_refspec_as_one_argument() {
    let fake = FakeGit::new(vec![git_output(true, "", "")]);
    let inspector = GitInspector::with_runner(fake.clone());

    let output = inspector
        .fetch_origin(Path::new("/fixture/repo"), "main")
        .unwrap();

    assert!(output.success);
    assert_eq!(fake.calls(), vec![vec!["fetch", "origin", "main"]]);
}

#[test]
fn origin_behind_count_uses_the_local_base_to_remote_range() {
    let fake = FakeGit::new(vec![git_output(true, "3", "")]);
    let inspector = GitInspector::with_runner(fake.clone());

    let behind = inspector
        .commits_behind_origin(Path::new("/fixture/repo"), "main")
        .unwrap();

    assert_eq!(behind, 3);
    assert_eq!(
        fake.calls(),
        vec![vec!["rev-list", "--count", "main..origin/main"]]
    );
}

#[test]
fn ref_guard_rejects_option_like_values_before_process_execution() {
    let fake = FakeGit::new(Vec::new());
    let inspector = GitInspector::with_runner(fake.clone());

    assert!(inspector
        .fetch_origin(Path::new("/fixture/repo"), "--upload-pack=bad")
        .is_err());
    assert!(fake.calls().is_empty());
}
