use crate::git::inspection::GitCommandRunner;
use crate::git::pull_request::PullRequestRequest;
use crate::git::pull_request::{GhCommandRunner, GitHubPullRequestAdapter};
use crate::runs::RunRecord;
use crate::Result;

pub trait PullRequestFinalizer: Send + Sync {
    fn open(&self, request: &PullRequestRequest) -> Result<Option<String>>;
}

pub fn request_for_run(run: &RunRecord) -> Option<PullRequestRequest> {
    Some(PullRequestRequest {
        repository: run.repo.clone()?,
        branch: run.branch.clone()?,
        base: run.base_ref.clone().or_else(|| run.base.clone()),
        title: run
            .prompt
            .lines()
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(72).collect()),
        result_text: run.result_text.clone(),
        run_id: Some(run.id.clone()),
        profile: Some(run.account()),
        expected_head_sha: None,
        draft: true,
    })
}

impl<G: GitCommandRunner + Send + Sync, H: GhCommandRunner> PullRequestFinalizer
    for GitHubPullRequestAdapter<G, H>
{
    fn open(&self, request: &PullRequestRequest) -> Result<Option<String>> {
        Ok(GitHubPullRequestAdapter::open(self, request)?
            .url()
            .map(str::to_owned))
    }
}
