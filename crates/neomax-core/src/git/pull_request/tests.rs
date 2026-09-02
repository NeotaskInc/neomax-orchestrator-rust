use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::super::inspection::{GitCommandOutput, GitCommandRunner};
use super::ports::{GhCommandOutput, GhCommandRunner};
use super::{receipt_body, GitHubPullRequestAdapter, PullRequestOutcome, PullRequestRequest};
use crate::{Error, Result};

#[derive(Clone)]
struct FakeGit {
    root: PathBuf,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    ahead: u64,
    remote: bool,
    push_success: bool,
}

impl FakeGit {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            calls: Arc::new(Mutex::new(Vec::new())),
            ahead: 1,
            remote: true,
            push_success: true,
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl GitCommandRunner for FakeGit {
    fn run(&self, _cwd: &Path, args: &[String]) -> Result<GitCommandOutput> {
        self.calls.lock().unwrap().push(args.to_vec());
        let output = |success: bool, stdout: &str, stderr: &str| GitCommandOutput {
            success,
            stdout: stdout.into(),
            stderr: stderr.into(),
        };
        match args {
            [command, flag] if command == "rev-parse" && flag == "--show-toplevel" => {
                Ok(output(true, &self.root.to_string_lossy(), ""))
            }
            [command] if command == "remote" => {
                Ok(output(true, if self.remote { "origin\n" } else { "" }, ""))
            }
            [command, flag, value]
                if command == "symbolic-ref"
                    && flag == "--short"
                    && value == "refs/remotes/origin/HEAD" =>
            {
                Ok(output(true, "origin/main", ""))
            }
            [command, flag, reference] if command == "rev-parse" && flag == "--verify" => {
                Ok(output(reference != "missing^{commit}", "sha", ""))
            }
            [command, flag, _range] if command == "rev-list" && flag == "--count" => {
                Ok(output(true, &self.ahead.to_string(), ""))
            }
            [command, flag, _base, _branch]
                if command == "merge-base" && flag == "--is-ancestor" =>
            {
                Ok(output(false, "", ""))
            }
            [command, flag, remote, _branch]
                if command == "push" && flag == "-u" && remote == "origin" =>
            {
                Ok(output(self.push_success, "", "push failed"))
            }
            _ => Err(Error::Message(format!(
                "unexpected fake git args: {args:?}"
            ))),
        }
    }
}

#[derive(Clone)]
enum GhMode {
    Existing,
    Create,
    Race,
}

#[derive(Clone)]
struct FakeGh {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    mode: GhMode,
}

impl FakeGh {
    fn new(mode: GhMode) -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            mode,
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

impl GhCommandRunner for FakeGh {
    fn run(&self, _cwd: &Path, args: &[String]) -> Result<GhCommandOutput> {
        self.calls.lock().unwrap().push(args.to_vec());
        let view = args.get(1).map(String::as_str) == Some("view");
        let create = args.get(1).map(String::as_str) == Some("create");
        let count = self.calls.lock().unwrap().len();
        let output = |success: bool, stdout: &str, stderr: &str| GhCommandOutput {
            success,
            stdout: stdout.into(),
            stderr: stderr.into(),
        };
        match (&self.mode, view, create, count) {
            (GhMode::Existing, true, false, _) | (GhMode::Race, true, false, 3) => Ok(output(
                true,
                r#"{"url":"https://github.com/acme/repo/pull/7","state":"OPEN","number":7}"#,
                "",
            )),
            (GhMode::Create, true, false, _) | (GhMode::Race, true, false, 1) => {
                Ok(output(false, "", "no pull requests found"))
            }
            (GhMode::Create, false, true, _) => {
                Ok(output(true, "https://github.com/acme/repo/pull/8\n", ""))
            }
            (GhMode::Race, false, true, 2) => Ok(output(false, "", "already exists")),
            _ => Err(Error::Message(format!("unexpected fake gh args: {args:?}"))),
        }
    }
}

fn request(root: &Path) -> PullRequestRequest {
    PullRequestRequest::branch(root, "feature/task")
        .result_text("Implement the task")
        .run_id("run-1")
        .profile("2")
}

#[test]
fn creates_a_draft_after_pushing_without_staging_or_committing() {
    let root = PathBuf::from("/fixture/repo");
    let git = FakeGit::new(&root);
    let gh = FakeGh::new(GhMode::Create);
    let adapter = GitHubPullRequestAdapter::with_ports(git.clone(), gh.clone());

    let outcome = adapter.open(&request(&root).title("Task title")).unwrap();
    let PullRequestOutcome::Opened(receipt) = outcome else {
        panic!("expected created PR");
    };
    assert_eq!(receipt.url, "https://github.com/acme/repo/pull/8");
    assert_eq!(receipt.base, "main");
    assert!(!receipt.reused);
    let calls = git.calls();
    assert!(calls
        .iter()
        .any(|call| call == &["push", "-u", "origin", "feature/task"]));
    assert!(!calls
        .iter()
        .any(|call| call.first().is_some_and(|value| value == "add")));
    assert!(!calls
        .iter()
        .any(|call| call.first().is_some_and(|value| value == "commit")));
    let gh_calls = gh.calls();
    let create = gh_calls
        .iter()
        .find(|call| call.get(1).map(String::as_str) == Some("create"))
        .unwrap();
    assert!(create.contains(&"--draft".into()));
    let body_index = create.iter().position(|value| value == "--body").unwrap();
    assert!(create[body_index + 1].contains("base:main"));
    assert!(create[body_index + 1].contains("run-1"));
}

#[test]
fn blank_result_text_defaults_the_title_to_the_branch() {
    let root = PathBuf::from("/fixture/repo");
    let git = FakeGit::new(&root);
    let gh = FakeGh::new(GhMode::Create);
    let adapter = GitHubPullRequestAdapter::with_ports(git, gh.clone());

    adapter.open(&request(&root).result_text("\n  ")).unwrap();

    let create = gh
        .calls()
        .into_iter()
        .find(|call| call.get(1).map(String::as_str) == Some("create"))
        .unwrap();
    let title_index = create.iter().position(|value| value == "--title").unwrap();
    assert_eq!(create[title_index + 1], "feature/task");
}

#[test]
fn existing_pr_is_returned_without_push_or_create() {
    let root = PathBuf::from("/fixture/repo");
    let git = FakeGit::new(&root);
    let gh = FakeGh::new(GhMode::Existing);
    let adapter = GitHubPullRequestAdapter::with_ports(git.clone(), gh.clone());
    let outcome = adapter.open(&request(&root)).unwrap();
    let PullRequestOutcome::Existing(receipt) = outcome else {
        panic!("expected existing PR");
    };
    assert_eq!(receipt.number, Some(7));
    assert!(receipt.reused);
    assert!(!git
        .calls()
        .iter()
        .any(|call| call.first() == Some(&"push".into())));
    assert_eq!(gh.calls().len(), 1);
}

#[test]
fn create_race_rechecks_the_branch_and_returns_the_winner() {
    let root = PathBuf::from("/fixture/repo");
    let git = FakeGit::new(&root);
    let gh = FakeGh::new(GhMode::Race);
    let adapter = GitHubPullRequestAdapter::with_ports(git, gh.clone());
    let outcome = adapter.open(&request(&root)).unwrap();
    assert!(matches!(outcome, PullRequestOutcome::Existing(receipt) if receipt.number == Some(7)));
    assert_eq!(gh.calls().len(), 3);
}

#[test]
fn a_branch_with_no_commits_ahead_is_a_safe_noop() {
    let root = PathBuf::from("/fixture/repo");
    let mut git = FakeGit::new(&root);
    git.ahead = 0;
    let gh = FakeGh::new(GhMode::Create);
    let adapter = GitHubPullRequestAdapter::with_ports(git.clone(), gh);
    let outcome = adapter.open(&request(&root)).unwrap();
    assert_eq!(
        outcome,
        PullRequestOutcome::AlreadyMerged {
            branch: "feature/task".into(),
            base: "main".into(),
        }
    );
    assert!(!git
        .calls()
        .iter()
        .any(|call| call.first() == Some(&"push".into())));
}

#[test]
fn no_remote_fails_before_gh_is_called() {
    let root = PathBuf::from("/fixture/repo");
    let mut git = FakeGit::new(&root);
    git.remote = false;
    let gh = FakeGh::new(GhMode::Create);
    let adapter = GitHubPullRequestAdapter::with_ports(git, gh.clone());
    let error = adapter.open(&request(&root)).unwrap_err();
    assert!(matches!(error, Error::Conflict(message) if message.contains("no Git remote")));
    assert!(gh.calls().is_empty());
}

#[test]
fn expected_head_guard_stops_before_existing_pr_or_push() {
    let root = PathBuf::from("/fixture/repo");
    let git = FakeGit::new(&root);
    let gh = FakeGh::new(GhMode::Create);
    let adapter = GitHubPullRequestAdapter::with_ports(git.clone(), gh.clone());

    let outcome = adapter
        .open(&request(&root).expected_head_sha("expected-sha"))
        .unwrap();

    assert_eq!(
        outcome,
        PullRequestOutcome::Stopped {
            branch: "feature/task".into(),
            expected: "expected-sha".into(),
            actual: "sha".into(),
        }
    );
    assert!(!git
        .calls()
        .iter()
        .any(|call| call.first() == Some(&"push".into())));
    assert!(gh.calls().is_empty());
}

#[test]
fn receipt_body_is_product_safe_and_contains_run_identity() {
    let body = receipt_body(&request(Path::new("/fixture/repo")));
    assert!(body.contains("Implement the task"));
    assert!(body.contains("run-1"));
    assert!(body.contains("branch `feature/task`"));
    assert!(body.contains("<!-- neomax:run:run-1"));
}
