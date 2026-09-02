use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

use super::runner::{require_success, GitCommandOutput, GitCommandRunner, ProcessGitRunner};
use super::types::{GitInspection, GitInspectionRequest};

#[derive(Debug, Clone, Copy, Default)]
pub struct GitInspector<R = ProcessGitRunner> {
    runner: R,
}

impl GitInspector<ProcessGitRunner> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R> GitInspector<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: GitCommandRunner> GitInspector<R> {
    pub fn inspect(&self, request: &GitInspectionRequest) -> Result<GitInspection> {
        let repository_root = self.repository_root(&request.repository)?;
        let branch = match request.branch.as_deref() {
            Some(branch) => validated_ref(branch, "branch")?.to_string(),
            None => self.current_branch(&repository_root)?,
        };
        let head_sha = self.resolve_ref(&repository_root, &branch, "branch")?;
        let (base, base_ref) = self.resolve_base(&repository_root, request.base.as_deref())?;
        let base_sha = self.resolve_ref(&repository_root, &base_ref, "base")?;
        let branch_is_ancestor_of_base = self.is_ancestor(&repository_root, &branch, &base_ref)?;
        let ahead = self.commits_ahead(&repository_root, &base_ref, &branch)?;

        Ok(GitInspection {
            repository_root,
            branch,
            base,
            base_ref,
            head_sha,
            base_sha,
            branch_is_ancestor_of_base,
            ahead,
        })
    }

    pub fn repository_root(&self, cwd: &Path) -> Result<PathBuf> {
        let output = self.run(cwd, &["rev-parse", "--show-toplevel"])?;
        if !output.success || output.stdout.is_empty() {
            return Err(Error::Message(format!(
                "not a Git repository: {}",
                display_error(&output)
            )));
        }
        Ok(PathBuf::from(output.stdout))
    }

    pub fn current_branch(&self, repository: &Path) -> Result<String> {
        let output = self.run(repository, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        if !output.success || output.stdout.is_empty() {
            return Err(Error::Message(
                "repository is in detached HEAD state or has no current branch".into(),
            ));
        }
        Ok(output.stdout)
    }

    pub fn default_base(&self, repository: &Path) -> Result<String> {
        let remote_head = self.run(
            repository,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        )?;
        if remote_head.success {
            let candidate = remote_head.stdout.trim();
            if !candidate.is_empty() {
                let local = candidate.strip_prefix("origin/").unwrap_or(candidate);
                if self.ref_exists(repository, local)? {
                    return Ok(local.to_owned());
                }
                if self.ref_exists(repository, candidate)? {
                    return Ok(candidate.to_owned());
                }
            }
        }

        for candidate in ["main", "master"] {
            if self.ref_exists(repository, candidate)? {
                return Ok(candidate.to_owned());
            }
            let remote = format!("origin/{candidate}");
            if self.ref_exists(repository, &remote)? {
                return Ok(remote);
            }
        }

        let current = self.run(repository, &["branch", "--show-current"])?;
        if current.success {
            let candidate = current.stdout.trim();
            if !candidate.is_empty() && self.ref_exists(repository, candidate)? {
                return Ok(candidate.to_owned());
            }
        }

        let branches = require_success(
            self.run(
                repository,
                &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
            )?,
            &format!("list branches in {}", repository.display()),
        )?;
        branches
            .stdout
            .lines()
            .map(str::trim)
            .find(|candidate| !candidate.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::NotFound("repository has no default branch".into()))
    }

    pub fn ref_exists(&self, repository: &Path, reference: &str) -> Result<bool> {
        let reference = validated_ref(reference, "Git ref")?;
        let output = self.run(
            repository,
            &["rev-parse", "--verify", &commit_ref(reference)],
        )?;
        Ok(output.success && !output.stdout.is_empty())
    }

    pub fn branch_checked_out(&self, repository: &Path, branch: &str) -> Result<bool> {
        let branch = validated_ref(branch, "branch")?;
        let output = require_success(
            self.run(repository, &["worktree", "list", "--porcelain"])?,
            &format!("inspect worktrees for {}", repository.display()),
        )?;
        let expected = format!("refs/heads/{branch}");
        Ok(output
            .stdout
            .lines()
            .filter_map(|line| line.strip_prefix("branch "))
            .any(|value| value == expected))
    }

    pub fn commits_ahead(&self, repository: &Path, base: &str, branch: &str) -> Result<u64> {
        let base = validated_ref(base, "base")?;
        let branch = validated_ref(branch, "branch")?;
        let range = format!("{base}..{branch}");
        let output = require_success(
            self.run(repository, &["rev-list", "--count", &range])?,
            &format!("compare {branch} with base {base}"),
        )?;
        output.stdout.parse::<u64>().map_err(|error| {
            Error::Message(format!("Git returned an invalid commit count: {error}"))
        })
    }

    pub fn worktree_registered(&self, repository: &Path, path: &Path) -> Result<bool> {
        if crate::io::is_rooted_but_not_absolute(repository)
            || crate::io::is_rooted_but_not_absolute(path)
        {
            return Err(Error::InvalidArgument(
                "worktree paths must not be Windows partial roots".into(),
            ));
        }
        let output = require_success(
            self.run(repository, &["worktree", "list", "--porcelain"])?,
            &format!("inspect worktrees for {}", repository.display()),
        )?;
        let expected = canonical_or_lexical(path);
        Ok(output
            .stdout
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(Path::new)
            .filter_map(|candidate| resolved_git_path(repository, candidate))
            .any(|candidate| candidate == expected))
    }

    /// Resolve a branch or commit without invoking any shell interpolation.
    /// Callers use this for SHA guards that must run before a push or PR call.
    pub fn resolve_commit(&self, repository: &Path, reference: &str, kind: &str) -> Result<String> {
        let reference = validated_ref(reference, kind)?;
        self.resolve_ref(repository, reference, kind)
    }

    /// Refresh one origin branch. The command runner remains injectable so
    /// pre-merge checks can prove the exact argument boundary without a remote.
    pub fn fetch_origin(&self, repository: &Path, base: &str) -> Result<GitCommandOutput> {
        let base = validated_ref(base, "base")?;
        self.run(repository, &["fetch", "origin", base])
    }

    /// Count commits present on origin but absent from the local base.
    pub fn commits_behind_origin(&self, repository: &Path, base: &str) -> Result<u64> {
        let base = validated_ref(base, "base")?;
        self.commits_ahead(repository, base, &format!("origin/{base}"))
    }

    fn resolve_base(&self, repository: &Path, explicit: Option<&str>) -> Result<(String, String)> {
        if let Some(base) = explicit {
            let base = validated_ref(base, "base")?;
            let base_ref = self
                .first_existing_ref(repository, &[base.to_string(), format!("origin/{base}")])?
                .ok_or_else(|| Error::NotFound(format!("base '{base}' not found")))?;
            return Ok((base.to_string(), base_ref));
        }

        if let Some(remote_head) = self.remote_default_branch(repository)? {
            let logical = remote_head
                .strip_prefix("origin/")
                .unwrap_or(&remote_head)
                .to_string();
            if let Some(base_ref) =
                self.first_existing_ref(repository, &[logical.clone(), remote_head.clone()])?
            {
                return Ok((logical, base_ref));
            }
        }

        for logical in ["main", "master"] {
            if let Some(base_ref) = self.first_existing_ref(
                repository,
                &[logical.to_string(), format!("origin/{logical}")],
            )? {
                return Ok((logical.to_string(), base_ref));
            }
        }

        Err(Error::NotFound(
            "could not determine a local default base branch".into(),
        ))
    }

    fn remote_default_branch(&self, repository: &Path) -> Result<Option<String>> {
        let output = self.run(
            repository,
            &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        )?;
        if output.success && !output.stdout.is_empty() {
            return Ok(Some(output.stdout));
        }
        Ok(None)
    }

    fn first_existing_ref(&self, repository: &Path, refs: &[String]) -> Result<Option<String>> {
        for reference in refs {
            if self.ref_exists(repository, reference)? {
                return Ok(Some(reference.clone()));
            }
        }
        Ok(None)
    }

    fn resolve_ref(&self, repository: &Path, reference: &str, kind: &str) -> Result<String> {
        let reference = validated_ref(reference, kind)?;
        let output = self.run(
            repository,
            &["rev-parse", "--verify", &commit_ref(reference)],
        )?;
        if !output.success || output.stdout.is_empty() {
            return Err(Error::NotFound(format!("{kind} '{reference}' not found")));
        }
        Ok(output.stdout)
    }

    fn is_ancestor(&self, repository: &Path, branch: &str, base: &str) -> Result<bool> {
        let output = self.run(repository, &["merge-base", "--is-ancestor", branch, base])?;
        if output.success {
            return Ok(true);
        }
        if output.stderr.is_empty() {
            return Ok(false);
        }
        Err(Error::Message(format!(
            "could not inspect ancestor relation: {}",
            display_error(&output)
        )))
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Result<GitCommandOutput> {
        let owned = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        self.runner.run(cwd, &owned)
    }
}

fn resolved_git_path(repository: &Path, candidate: &Path) -> Option<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(candidate) {
        return None;
    }
    Some(if candidate.is_absolute() {
        canonical_or_lexical(candidate)
    } else {
        canonical_or_lexical(&repository.join(candidate))
    })
}

fn required_name<'a>(value: &'a str, kind: &str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty()
        || value.starts_with('-')
        || value.contains('\0')
        || value.chars().any(char::is_control)
    {
        return Err(Error::InvalidArgument(format!("{kind} is invalid")));
    }
    Ok(value)
}

fn validated_ref<'a>(value: &'a str, kind: &str) -> Result<&'a str> {
    let value = required_name(value, kind)?;
    crate::git::workspace::validate_ref_name(value)?;
    Ok(value)
}

fn commit_ref(reference: &str) -> String {
    format!("{reference}^{{commit}}")
}

fn display_error(output: &GitCommandOutput) -> String {
    if !output.stderr.is_empty() {
        output.stderr.clone()
    } else if !output.stdout.is_empty() {
        output.stdout.clone()
    } else {
        "git command failed".into()
    }
}

fn canonical_or_lexical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| lexical_absolute(path, Path::new(".")))
}

fn lexical_absolute(path: &Path, base: &Path) -> PathBuf {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    let mut result = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn git_worktree_paths_reject_windows_partial_roots() {
        let repository = Path::new(r"C:\fixture\repo");
        assert!(resolved_git_path(repository, Path::new(r"\outside")).is_none());
        assert!(resolved_git_path(repository, Path::new(r"C:outside")).is_none());
    }
}
