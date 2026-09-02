use std::ffi::OsStr;
use std::path::Path;

use crate::git::invoke;
use crate::{Error, Result};

use super::types::AllocationStatus;

pub fn validate_plan_id(value: &str) -> Result<()> {
    validate_component(value, "plan id")
}

pub fn validate_ref_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "@"
        || value.len() > 1024
        || value.starts_with('-')
        || value.starts_with('/')
        || value.ends_with('/')
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || value.contains("@{")
        || value.chars().any(|character| {
            character.is_ascii_control()
                || matches!(character, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
        || value.split('/').any(|component| component.is_empty())
        || value.split('/').any(|component| {
            component.starts_with('.') || component.ends_with('.') || component.ends_with(".lock")
        })
    {
        return Err(Error::InvalidArgument(format!(
            "invalid Git ref name {value:?}"
        )));
    }
    Ok(())
}

pub fn generated_integration_branch(plan_id: &str) -> Result<String> {
    validate_plan_id(plan_id)?;
    Ok(format!("neomax/int-{plan_id}"))
}

pub fn generated_part_branch(plan_id: &str, part_id: &str) -> Result<String> {
    validate_plan_id(plan_id)?;
    validate_component(part_id, "part id")?;
    Ok(format!("neomax/{plan_id}-{part_id}"))
}

pub fn resolve_default_branch(repository: &Path) -> Result<String> {
    let remote_head = invoke(
        repository,
        [
            OsStr::new("symbolic-ref"),
            OsStr::new("--quiet"),
            OsStr::new("--short"),
            OsStr::new("refs/remotes/origin/HEAD"),
        ],
    )?;
    if remote_head.success {
        let candidate = remote_head.stdout_text();
        if !candidate.is_empty() {
            let local = candidate.strip_prefix("origin/").unwrap_or(&candidate);
            if branch_exists(repository, local)? {
                return Ok(local.into());
            }
            validate_ref_name(&candidate)?;
            return Ok(candidate);
        }
    }

    for candidate in ["main", "master"] {
        if branch_exists(repository, candidate)? {
            return Ok(candidate.into());
        }
        let remote = format!("origin/{candidate}");
        if reference_exists(repository, &remote)? {
            return Ok(remote);
        }
    }

    let current = invoke(
        repository,
        [OsStr::new("branch"), OsStr::new("--show-current")],
    )?;
    if current.success {
        let candidate = current.stdout_text();
        if !candidate.is_empty() {
            validate_ref_name(&candidate)?;
            return Ok(candidate);
        }
    }

    let branches = invoke(
        repository,
        [
            OsStr::new("for-each-ref"),
            OsStr::new("--format=%(refname:short)"),
            OsStr::new("refs/heads"),
        ],
    )?;
    if branches.success {
        if let Some(candidate) = String::from_utf8_lossy(&branches.stdout)
            .lines()
            .map(str::trim)
            .find(|candidate| !candidate.is_empty())
        {
            validate_ref_name(candidate)?;
            return Ok(candidate.into());
        }
    }
    Err(Error::NotFound("repository has no default branch".into()))
}

pub fn branch_exists(repository: &Path, branch: &str) -> Result<bool> {
    validate_ref_name(branch)?;
    let reference = format!("refs/heads/{branch}");
    let result = invoke(
        repository,
        [
            OsStr::new("show-ref"),
            OsStr::new("--verify"),
            OsStr::new("--quiet"),
            OsStr::new(&reference),
        ],
    )?;
    Ok(result.success)
}

pub fn branch_commit(repository: &Path, branch: &str) -> Result<String> {
    validate_ref_name(branch)?;
    let revision = format!("{branch}^{{commit}}");
    let result = invoke(
        repository,
        [
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new(&revision),
        ],
    )?;
    if !result.success {
        return Err(Error::NotFound(format!("branch {branch}")));
    }
    Ok(result.stdout_text())
}

pub(crate) fn ensure_branch(
    repository: &Path,
    branch: &str,
    start_point: &str,
) -> Result<AllocationStatus> {
    validate_ref_name(branch)?;
    validate_ref_name(start_point)?;
    if branch_exists(repository, branch)? {
        branch_commit(repository, branch)?;
        return Ok(AllocationStatus::Reused);
    }
    if !reference_exists(repository, start_point)? {
        return Err(Error::NotFound(format!("base ref {start_point}")));
    }
    let result = invoke(
        repository,
        [
            OsStr::new("branch"),
            OsStr::new("--"),
            OsStr::new(branch),
            OsStr::new(start_point),
        ],
    )?;
    if !result.success {
        if branch_exists(repository, branch)? {
            branch_commit(repository, branch)?;
            return Ok(AllocationStatus::Reused);
        }
        return Err(Error::Message(result.stderr_text()));
    }
    Ok(AllocationStatus::Created)
}

fn reference_exists(repository: &Path, reference: &str) -> Result<bool> {
    validate_ref_name(reference)?;
    let revision = format!("{reference}^{{commit}}");
    let result = invoke(
        repository,
        [
            OsStr::new("rev-parse"),
            OsStr::new("--verify"),
            OsStr::new(&revision),
        ],
    )?;
    Ok(result.success)
}

fn validate_component(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
    {
        return Err(Error::InvalidArgument(format!(
            "{label} must use [A-Za-z0-9._-] without path traversal"
        )));
    }
    Ok(())
}
