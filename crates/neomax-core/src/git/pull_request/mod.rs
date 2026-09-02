mod adapter;
mod ports;
mod types;

pub use adapter::{receipt_body, GitHubPullRequestAdapter};
pub use ports::{
    GhCommandOutput, GhCommandRunner, ProcessGhRunner, GH_COMMAND_TIMEOUT, MAX_GH_OUTPUT_BYTES,
};
pub use types::{ExistingPullRequest, PullRequestOutcome, PullRequestReceipt, PullRequestRequest};

#[cfg(test)]
mod tests;
