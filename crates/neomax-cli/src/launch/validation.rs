use super::types::LaunchOptions;
use anyhow::{Error as AnyhowError, Result, bail};

use neomax_core::Error as CoreError;

pub(crate) const MAX_RUN_ID_CHARS: usize = 128;
pub(crate) const MAX_TAG_CHARS: usize = 120;

/// Goals are persisted in run records and passed through provider prompts, so
/// keep the public launch contract bounded by Unicode scalar values rather
/// than bytes.
pub(crate) const MAX_GOAL_CHARS: usize = 4_000;

const THIN_BRIEF_WARNING: &str = "warning: worker task brief is thin; include an explicit scope and acceptance checks, or pass --brief to acknowledge a concise brief";

pub(crate) fn normalize_goal(goal: &mut Option<String>) -> Result<()> {
    let Some(goal) = goal.as_mut() else {
        return Ok(());
    };
    *goal = goal.trim().to_owned();
    if goal.is_empty() {
        bail!("--goal requires a non-empty condition");
    }
    if goal.chars().count() > MAX_GOAL_CHARS {
        bail!("--goal must be at most {MAX_GOAL_CHARS} Unicode characters");
    }
    Ok(())
}

pub(crate) fn validate_run_id(value: &str) -> Result<String> {
    let invalid = value.is_empty()
        || value != value.trim()
        || value.chars().count() > MAX_RUN_ID_CHARS
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if invalid {
        return Err(AnyhowError::new(CoreError::InvalidArgument(format!(
            "--run-id must be a safe identifier of at most {MAX_RUN_ID_CHARS} ASCII characters"
        ))));
    }
    Ok(value.to_owned())
}

pub(crate) fn validate_tag(value: &str) -> Result<String> {
    let invalid = value.trim().is_empty()
        || value.chars().count() > MAX_TAG_CHARS
        || value.contains("..")
        || value
            .chars()
            .any(|character| matches!(character, '/' | '\\'))
        || value.chars().any(char::is_control);
    if invalid {
        return Err(AnyhowError::new(CoreError::InvalidArgument(format!(
            "--tag must be a non-empty, printable, path-safe value of at most {MAX_TAG_CHARS} Unicode characters"
        ))));
    }
    Ok(value.to_owned())
}

pub(crate) fn validate_run_metadata(options: &LaunchOptions) -> Result<()> {
    if let Some(run_id) = options.run_id.as_deref() {
        validate_run_id(run_id)?;
    }
    if let Some(tag) = options.tag.as_deref() {
        validate_tag(tag)?;
    }
    Ok(())
}

pub(crate) fn validate_worker_task(worker_dispatch: bool, positionals: &[String]) -> Result<()> {
    if worker_dispatch && task_brief(positionals).is_none() {
        bail!("worker dispatch requires a non-empty task brief; --goal is not a task");
    }
    Ok(())
}

pub(crate) fn validate_effective_base(options: &LaunchOptions) -> Result<()> {
    if options.base.is_some() && options.no_worktree {
        bail!(
            "--base cannot be combined with --no-worktree or --plan because no effective base is applied"
        );
    }
    Ok(())
}

pub(crate) fn validate_solo_options(options: &LaunchOptions) -> Result<()> {
    if !options.solo {
        return Ok(());
    }
    if options.worker_dispatch {
        bail!("solo mode cannot dispatch a worker");
    }
    if options.worker_scope.is_some() {
        bail!("solo mode cannot select a worker scope");
    }
    if options.goal.is_some()
        || options.base.is_some()
        || options.open_pull_request
        || options.plan_mode
        || options.dedicated
        || options.brief
        || options.no_failover
        || options.priority.is_some()
        || options.stall_min.is_some()
    {
        bail!(
            "solo mode accepts only provider selection, a model, a plain task, and foreground controls"
        );
    }
    if !options.foreground || options.detach {
        bail!("solo mode must remain a foreground provider session");
    }
    Ok(())
}

/// Return the nonblank task text represented by free-form positional args.
pub(crate) fn task_brief(positionals: &[String]) -> Option<String> {
    let task = positionals.join(" ");
    let task = task.trim();
    (!task.is_empty()).then_some(task.to_owned())
}

/// The reference CLI treats a thin worker brief as an advisory warning. A
/// brief that names both its scope and acceptance checks is sufficiently
/// bounded for this warning to stay silent. `--brief` is an explicit opt-out
/// for intentionally concise prompts.
pub(crate) fn thin_brief_warning(options: &LaunchOptions) -> Option<&'static str> {
    if !options.worker_dispatch || options.brief {
        return None;
    }
    let task = task_brief(&options.positionals)?;
    let lower = task.to_lowercase();
    let scoped = [
        "scope:",
        "scope=",
        "files:",
        "file:",
        "paths:",
        "path:",
        "area:",
        "areas:",
        "affected:",
        "files to touch",
        "touch:",
        "do-not-touch:",
        "do not touch",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let accepted = [
        "acceptance:",
        "acceptance=",
        "acceptance criteria",
        "acceptance checks",
        "acceptance tests",
        "done when",
        "verify:",
        "verification:",
        "checks:",
        "tests:",
        "expected:",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    (!scoped || !accepted).then_some(THIN_BRIEF_WARNING)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(task: &[&str]) -> LaunchOptions {
        LaunchOptions {
            worker_dispatch: true,
            positionals: task.iter().map(|value| (*value).into()).collect(),
            ..LaunchOptions::default()
        }
    }

    #[test]
    fn goals_are_trimmed_and_capped_by_unicode_characters() {
        let mut goal = Some("  objective  ".to_owned());
        normalize_goal(&mut goal).unwrap();
        assert_eq!(goal.as_deref(), Some("objective"));

        let mut unicode_goal = Some("é".repeat(MAX_GOAL_CHARS));
        normalize_goal(&mut unicode_goal).unwrap();
        unicode_goal = Some(format!("{}é", unicode_goal.unwrap()));
        let error = normalize_goal(&mut unicode_goal).expect_err("goal over the Unicode cap");
        assert!(error.to_string().contains("Unicode characters"));
    }

    #[test]
    fn worker_requires_a_nonblank_positional_task_even_when_goal_exists_elsewhere() {
        assert!(validate_worker_task(true, &[]).is_err());
        assert!(validate_worker_task(true, &["   ".into()]).is_err());
        assert!(validate_worker_task(false, &[]).is_ok());
    }

    #[test]
    fn thin_brief_warning_is_suppressed_by_brief_or_explicit_scope_and_acceptance() {
        let mut concise = options(&["fix", "the", "bug"]);
        assert!(thin_brief_warning(&concise).is_some());

        concise.brief = true;
        assert!(thin_brief_warning(&concise).is_none());

        let scoped = options(&[
            "Objective:",
            "fix",
            "the",
            "bug;",
            "Scope:",
            "src/bug.rs;",
            "Acceptance:",
            "tests pass",
        ]);
        assert!(thin_brief_warning(&scoped).is_none());
    }

    #[test]
    fn a_base_is_rejected_when_execution_stays_in_the_current_checkout() {
        let options = LaunchOptions {
            base: Some("main".into()),
            no_worktree: true,
            ..LaunchOptions::default()
        };
        let error = validate_effective_base(&options).expect_err("base would be ineffective");
        assert!(error.to_string().contains("effective base"));
    }
}
