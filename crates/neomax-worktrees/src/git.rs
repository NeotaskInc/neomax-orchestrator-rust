use std::path::Path;

use neomax_core::Result;
use neomax_core::git::GitInspector;
pub use neomax_core::git::GitProcessConfig;
pub use neomax_core::git::inspection::{
    ConfiguredGitRunner, GitCommandOutput as GitOutput, GitCommandRunner as GitRunner,
    ProcessGitRunner, require_success,
};

pub use neomax_core::git::{DEFAULT_COMMAND_TIMEOUT, MAX_COMMAND_OUTPUT_BYTES};

pub type ConfiguredGit = ConfiguredGitRunner;
#[cfg(not(test))]
pub use neomax_core::git::inspection::ProcessGitRunner as ProcessGit;

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessGit;

#[cfg(test)]
impl GitRunner for ProcessGit {
    fn run(&self, cwd: &Path, args: &[String]) -> Result<GitOutput> {
        let mut safe_args = hermetic_args();
        safe_args.extend(args.iter().cloned());
        let output = std::process::Command::new("git")
            .args(safe_args)
            .current_dir(cwd)
            .env_clear()
            .env("HOME", cwd.join(".neomax-git-fixture/home"))
            .env("XDG_CONFIG_HOME", cwd.join(".neomax-git-fixture/config"))
            .env("GIT_CONFIG_GLOBAL", cwd.join(".neomax-git-fixture/global"))
            .env("GIT_CONFIG_SYSTEM", cwd.join(".neomax-git-fixture/system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env(
                "GIT_ASKPASS",
                cwd.join(".neomax-git-fixture/missing-askpass"),
            )
            .env(
                "SSH_ASKPASS",
                cwd.join(".neomax-git-fixture/missing-ssh-askpass"),
            )
            .env("GIT_EDITOR", hermetic_editor_command())
            .env("GIT_SEQUENCE_EDITOR", hermetic_editor_command())
            .env("GIT_PAGER", hermetic_pager_command())
            .env("PAGER", hermetic_pager_command())
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("PATH", fixture_path())
            .output()
            .map_err(neomax_core::Error::Io)?;
        Ok(GitOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().into(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().into(),
        })
    }
}

#[cfg(test)]
fn hermetic_editor_command() -> &'static str {
    #[cfg(windows)]
    {
        "cmd.exe /d /c exit 0"
    }
    #[cfg(not(windows))]
    {
        ":"
    }
}

#[cfg(test)]
const fn hermetic_pager_command() -> &'static str {
    ""
}

#[cfg(test)]
fn hermetic_args() -> Vec<String> {
    [
        "-c",
        "core.hooksPath=__neomax_no_test_hooks__",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "tag.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.sshCommand=",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
fn fixture_path() -> std::ffi::OsString {
    #[cfg(windows)]
    {
        std::env::var_os("PATH").unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        std::env::join_paths([Path::new("/usr/bin"), Path::new("/bin")])
            .expect("fixture PATH entries are valid")
    }
}

pub fn args<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    values.into_iter().map(Into::into).collect()
}

pub fn checked_text<G: GitRunner>(git: &G, cwd: &Path, values: &[String]) -> Result<String> {
    require_success(git.run(cwd, values)?, "git command").map(|output| output.stdout)
}

pub fn ref_exists<G: GitRunner>(git: &G, repository: &Path, reference: &str) -> Result<bool> {
    GitInspector::with_runner(git).ref_exists(repository, reference)
}

pub fn ref_commit<G: GitRunner>(git: &G, repository: &Path, reference: &str) -> Result<String> {
    GitInspector::with_runner(git).resolve_commit(repository, reference, "Git ref")
}

pub fn default_base<G: GitRunner>(git: &G, repository: &Path) -> Result<String> {
    GitInspector::with_runner(git).default_base(repository)
}

pub fn branch_checked_out<G: GitRunner>(git: &G, repository: &Path, branch: &str) -> Result<bool> {
    GitInspector::with_runner(git).branch_checked_out(repository, branch)
}

pub fn commits_ahead<G: GitRunner>(
    git: &G,
    repository: &Path,
    base: &str,
    branch: &str,
) -> Result<u64> {
    GitInspector::with_runner(git).commits_ahead(repository, base, branch)
}

pub fn worktree_registered<G: GitRunner>(git: &G, repository: &Path, path: &Path) -> Result<bool> {
    GitInspector::with_runner(git).worktree_registered(repository, path)
}

#[cfg(test)]
mod tests {
    use super::{hermetic_editor_command, hermetic_pager_command};

    #[test]
    fn hermetic_git_helpers_are_portable_and_noninteractive() {
        #[cfg(windows)]
        assert_eq!(hermetic_editor_command(), "cmd.exe /d /c exit 0");
        #[cfg(not(windows))]
        assert_eq!(hermetic_editor_command(), ":");
        assert!(hermetic_pager_command().is_empty());
    }
}
