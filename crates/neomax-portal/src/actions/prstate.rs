use std::time::Duration;

use anyhow::{Result, bail};
use neomax_core::io::{LocalProcessRunner, ProcessOutput, ProcessRequest, ProcessRunner};
use neomax_core::providers::scrub_provider_process_request;
use serde_json::Value;

use crate::model::PrStateView;

const MAX_PR_URL_LENGTH: usize = 512;
const MAX_PR_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_PR_ERROR_BYTES: usize = 16 * 1024;
const PR_TIMEOUT: Duration = Duration::from_secs(15);

pub trait PrStateResolver: Send + Sync {
    fn resolve(&self, url: &str) -> Result<PrStateView>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GhPrStateResolver;

impl PrStateResolver for GhPrStateResolver {
    fn resolve(&self, url: &str) -> Result<PrStateView> {
        validate_pr_url(url)?;
        let output = bounded_gh_output(url);
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                crate::security::log_internal("pull request state lookup", &error);
                return Ok(PrStateView::unavailable(
                    url,
                    "pull request state unavailable",
                ));
            }
        };
        if output.timed_out {
            crate::security::log_internal(
                "pull request state lookup",
                &format!("command exceeded {} seconds", PR_TIMEOUT.as_secs()),
            );
            return Ok(PrStateView::unavailable(
                url,
                "pull request state unavailable",
            ));
        }
        if output.stdout_truncated || output.stderr_truncated {
            crate::security::log_internal("pull request state lookup", "output exceeded limit");
            return Ok(PrStateView::unavailable(
                url,
                "pull request state unavailable",
            ));
        }
        if !output.success {
            let message = String::from_utf8_lossy(&output.stderr);
            crate::security::log_internal("pull request state lookup", &message);
            return Ok(PrStateView::unavailable(
                url,
                "pull request state unavailable",
            ));
        }
        let value: Value = match serde_json::from_slice(&output.stdout) {
            Ok(value) => value,
            Err(error) => {
                crate::security::log_internal("pull request state response", &error);
                return Ok(PrStateView::unavailable(
                    url,
                    "pull request state unavailable",
                ));
            }
        };
        Ok(PrStateView {
            url: url.into(),
            available: true,
            state: value.get("state").and_then(Value::as_str).map(Into::into),
            is_draft: value
                .get("isDraft")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            merged: value.get("mergedAt").is_some_and(|value| !value.is_null()),
            error: None,
        })
    }
}

fn bounded_gh_output(url: &str) -> anyhow::Result<ProcessOutput> {
    let request = ProcessRequest::new("gh")
        .args(["pr", "view", url, "--json", "state,isDraft,mergedAt"])
        .env("GH_PAGER", "cat")
        .timeout(PR_TIMEOUT)
        .stdout_limit(MAX_PR_OUTPUT_BYTES)
        .stderr_limit(MAX_PR_ERROR_BYTES);
    let request = scrub_provider_process_request(request);
    Ok(LocalProcessRunner::default().capture(&request)?)
}

pub fn validate_pr_url(url: &str) -> Result<()> {
    if url.len() > MAX_PR_URL_LENGTH || !url.starts_with("https://github.com/") {
        bail!("PR URL must be a GitHub HTTPS pull-request URL")
    }
    let parts = url["https://github.com/".len()..]
        .split('/')
        .collect::<Vec<_>>();
    if parts.len() != 4
        || parts[2] != "pull"
        || parts[3].is_empty()
        || !parts[3].chars().all(|character| character.is_ascii_digit())
        || parts[..2].iter().any(|part| {
            part.is_empty()
                || matches!(*part, "." | "..")
                || !part.chars().all(safe_github_component)
        })
    {
        bail!("PR URL must be a GitHub HTTPS pull-request URL")
    }
    Ok(())
}

fn safe_github_component(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_canonical_github_pull_requests() {
        assert!(validate_pr_url("https://github.com/NeotaskInc/neomax/pull/42").is_ok());
        assert!(validate_pr_url("https://github.com/NeotaskInc/neomax/pull/42?x=1").is_err());
        assert!(validate_pr_url("https://github.com/../neomax/pull/42").is_err());
        assert!(validate_pr_url("https://evil.example/NeotaskInc/neomax/pull/42").is_err());
    }

    #[test]
    fn unavailable_pr_state_never_contains_command_output_on_invalid_url() {
        let resolver = GhPrStateResolver;
        assert!(resolver.resolve("https://example.test/pr/1").is_err());
    }
}
