use std::path::Path;

use serde_json::Value;

use crate::git::inspection::{GitCommandRunner, ProcessGitRunner};
use crate::{Error, Result};

use super::ports::{gh_failure, git_failure, GhCommandRunner, ProcessGhRunner};
use super::types::{
    ExistingPullRequest, PullRequestOutcome, PullRequestReceipt, PullRequestRequest,
};

const MAX_BRANCH_BYTES: usize = 512;
const MAX_BASE_BYTES: usize = 512;
const MAX_TITLE_BYTES: usize = 512;
const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedBase {
    logical: String,
    reference: String,
}

#[derive(Debug, Clone)]
pub struct GitHubPullRequestAdapter<G = ProcessGitRunner, H = ProcessGhRunner> {
    git: G,
    gh: H,
}

impl Default for GitHubPullRequestAdapter<ProcessGitRunner, ProcessGhRunner> {
    fn default() -> Self {
        Self {
            git: ProcessGitRunner,
            gh: ProcessGhRunner,
        }
    }
}

impl<G, H> GitHubPullRequestAdapter<G, H> {
    pub fn with_ports(git: G, gh: H) -> Self {
        Self { git, gh }
    }
}

impl<G: GitCommandRunner, H: GhCommandRunner> GitHubPullRequestAdapter<G, H> {
    pub fn open(&self, request: &PullRequestRequest) -> Result<PullRequestOutcome> {
        validate_request(request)?;
        let repository = self.repository_root(&request.repository)?;
        self.require_ref(&repository, &request.branch, "branch")?;

        if let Some(expected) = request.expected_head_sha.as_deref() {
            let actual = self
                .git
                .run(
                    &repository,
                    &args([
                        "rev-parse",
                        "--verify",
                        &format!("{}^{{commit}}", request.branch),
                    ]),
                )?
                .stdout
                .trim()
                .to_owned();
            if actual != expected {
                return Ok(PullRequestOutcome::Stopped {
                    branch: request.branch.clone(),
                    expected: expected.to_owned(),
                    actual,
                });
            }
        }

        self.require_remote(&repository)?;
        let base = self.resolve_base(&repository, request.base.as_deref())?;

        if let Some(existing) = self.existing_pr(&repository, &request.branch)? {
            return Ok(PullRequestOutcome::Existing(self.receipt(
                request,
                &base.logical,
                existing,
                true,
            )));
        }

        let ahead = self.commits_ahead(&repository, &base.reference, &request.branch)?;
        if ahead == 0 {
            return Ok(PullRequestOutcome::AlreadyMerged {
                branch: request.branch.clone(),
                base: base.logical,
            });
        }

        self.push(&repository, &request.branch)?;
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| default_title(request));
        let body = receipt_body_for_base(request, &base.logical);
        let mut args = vec![
            "pr".to_owned(),
            "create".to_owned(),
            "--head".to_owned(),
            request.branch.clone(),
            "--base".to_owned(),
            base.logical.clone(),
            "--title".to_owned(),
            title,
            "--body".to_owned(),
            body,
        ];
        if request.draft {
            args.push("--draft".to_owned());
        }
        let created = self.gh.run(&repository, &args)?;
        if !created.success {
            if let Some(existing) = self.existing_pr(&repository, &request.branch)? {
                return Ok(PullRequestOutcome::Existing(self.receipt(
                    request,
                    &base.logical,
                    existing,
                    true,
                )));
            }
            return Err(gh_failure(&created, "pr create"));
        }
        let url = parse_url(&created.stdout).ok_or_else(|| {
            Error::Message("gh pr create returned success without a pull-request URL".into())
        })?;
        Ok(PullRequestOutcome::Opened(PullRequestReceipt {
            url,
            number: None,
            state: Some("OPEN".into()),
            branch: request.branch.clone(),
            base: base.logical,
            reused: false,
        }))
    }

    fn repository_root(&self, repository: &Path) -> Result<std::path::PathBuf> {
        let output = self
            .git
            .run(repository, &args(["rev-parse", "--show-toplevel"]))?;
        if !output.success || output.stdout.trim().is_empty() {
            return Err(Error::Message(format!(
                "not a Git repository: {}",
                git_failure(&output, "repository root")
            )));
        }
        Ok(std::path::PathBuf::from(output.stdout.trim()))
    }

    fn require_remote(&self, repository: &Path) -> Result<()> {
        let output = self.git.run(repository, &args(["remote"]))?;
        if !output.success {
            return Err(git_failure(&output, "remote"));
        }
        if output.stdout.lines().all(|line| line.trim().is_empty()) {
            return Err(Error::Conflict("repository has no Git remote".into()));
        }
        Ok(())
    }

    fn resolve_base(&self, repository: &Path, explicit: Option<&str>) -> Result<ResolvedBase> {
        if let Some(explicit) = explicit {
            let logical = validate_ref(explicit, "base", MAX_BASE_BYTES)?.to_owned();
            let reference = self
                .first_existing_ref(repository, &[logical.clone(), format!("origin/{logical}")])?
                .ok_or_else(|| Error::NotFound(format!("base '{logical}' not found")))?;
            return Ok(ResolvedBase { logical, reference });
        }

        let default = self
            .git
            .run(
                repository,
                &args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]),
            )?
            .stdout
            .trim()
            .strip_prefix("origin/")
            .map(str::to_owned);
        let mut candidates = Vec::new();
        if let Some(default) = default {
            candidates.push(default);
        }
        candidates.extend(["main".into(), "master".into()]);
        for logical in candidates {
            if let Some(reference) = self
                .first_existing_ref(repository, &[logical.clone(), format!("origin/{logical}")])?
            {
                return Ok(ResolvedBase { logical, reference });
            }
        }
        Err(Error::NotFound(
            "could not determine a local default base branch".into(),
        ))
    }

    fn first_existing_ref(&self, repository: &Path, refs: &[String]) -> Result<Option<String>> {
        for reference in refs {
            if self.ref_exists(repository, reference)? {
                return Ok(Some(reference.clone()));
            }
        }
        Ok(None)
    }

    fn ref_exists(&self, repository: &Path, reference: &str) -> Result<bool> {
        let output = self.git.run(
            repository,
            &args(["rev-parse", "--verify", &format!("{reference}^{{commit}}")]),
        )?;
        Ok(output.success && !output.stdout.trim().is_empty())
    }

    fn require_ref(&self, repository: &Path, reference: &str, kind: &str) -> Result<()> {
        if self.ref_exists(repository, reference)? {
            Ok(())
        } else {
            Err(Error::NotFound(format!("{kind} '{reference}' not found")))
        }
    }

    fn commits_ahead(&self, repository: &Path, base: &str, branch: &str) -> Result<u64> {
        let output = self.git.run(
            repository,
            &args(["rev-list", "--count", &format!("{base}..{branch}")]),
        )?;
        if !output.success {
            return Err(git_failure(&output, "rev-list"));
        }
        output
            .stdout
            .trim()
            .parse()
            .map_err(|_| Error::Message("Git returned an invalid ahead count".into()))
    }

    fn push(&self, repository: &Path, branch: &str) -> Result<()> {
        let output = self
            .git
            .run(repository, &args(["push", "-u", "origin", branch]))?;
        if output.success {
            Ok(())
        } else {
            Err(git_failure(&output, "push"))
        }
    }

    fn existing_pr(&self, repository: &Path, branch: &str) -> Result<Option<ExistingPullRequest>> {
        let args = [
            "pr".to_owned(),
            "view".to_owned(),
            branch.to_owned(),
            "--json".to_owned(),
            "url,state,number".to_owned(),
            "--jq".to_owned(),
            "{url:.url,state:.state,number:.number}".to_owned(),
        ];
        let output = self.gh.run(repository, &args)?;
        if !output.success || output.stdout.trim().is_empty() {
            return Ok(None);
        }
        let value: Value = serde_json::from_str(&output.stdout).map_err(|error| {
            Error::Message(format!("gh pr view returned invalid JSON: {error}"))
        })?;
        let url = value
            .get("url")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Error::Message("gh pr view returned no pull-request URL".into()))?;
        Ok(Some(ExistingPullRequest {
            url: url.to_owned(),
            number: value.get("number").and_then(Value::as_u64),
            state: value
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }))
    }

    fn receipt(
        &self,
        request: &PullRequestRequest,
        base: &str,
        existing: ExistingPullRequest,
        reused: bool,
    ) -> PullRequestReceipt {
        PullRequestReceipt {
            url: existing.url,
            number: existing.number,
            state: existing.state,
            branch: request.branch.clone(),
            base: base.to_owned(),
            reused,
        }
    }
}

fn validate_request(request: &PullRequestRequest) -> Result<()> {
    if request.repository.as_os_str().is_empty() {
        return Err(Error::InvalidArgument("repository cannot be empty".into()));
    }
    validate_ref(&request.branch, "branch", MAX_BRANCH_BYTES)?;
    if let Some(base) = request.base.as_deref() {
        validate_ref(base, "base", MAX_BASE_BYTES)?;
    }
    if let Some(title) = request.title.as_deref() {
        validate_text(title, "title", MAX_TITLE_BYTES)?;
    }
    if let Some(body) = request.result_text.as_deref() {
        validate_text(body, "result text", MAX_BODY_BYTES)?;
    }
    if let Some(expected) = request.expected_head_sha.as_deref() {
        validate_ref(expected, "expected head SHA", MAX_BRANCH_BYTES)?;
    }
    Ok(())
}

fn validate_ref<'a>(value: &'a str, kind: &str, max: usize) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() || value.len() > max || value.contains('\0') || value.starts_with('-') {
        return Err(Error::InvalidArgument(format!("{kind} is invalid")));
    }
    Ok(value)
}

fn validate_text(value: &str, kind: &str, max: usize) -> Result<()> {
    if value.len() > max || value.contains('\0') {
        return Err(Error::InvalidArgument(format!(
            "{kind} is too large or invalid"
        )));
    }
    Ok(())
}

fn default_title(request: &PullRequestRequest) -> String {
    let source = request
        .result_text
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&request.branch);
    source
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(&request.branch)
        .trim()
        .chars()
        .take(72)
        .collect()
}

pub fn receipt_body(request: &PullRequestRequest) -> String {
    receipt_body_for_base(request, request.base.as_deref().unwrap_or("default"))
}

fn receipt_body_for_base(request: &PullRequestRequest, base: &str) -> String {
    let result = request.result_text.as_deref().unwrap_or("").trim();
    let run_id = request.run_id.as_deref().unwrap_or("standalone");
    let profile = request.profile.as_deref().unwrap_or("?");
    format!(
        "{result}\n\n---\n🤖 Delegated worker run `{run_id}` (account {profile}, branch `{}`).\nReview the diff before merging.\n\n<!-- neomax:run:{run_id} base:{} -->",
        request.branch, base
    )
}

fn parse_url(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|value| value.trim_matches(|character: char| "()[]{}<>,\"'".contains(character)))
        .find(|value| value.starts_with("https://") || value.starts_with("http://"))
        .map(str::to_owned)
}

fn args<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_owned).collect()
}
