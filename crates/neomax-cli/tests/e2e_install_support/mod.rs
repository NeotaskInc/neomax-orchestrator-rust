use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub(crate) const ALIASES: &[&str] = &[
    "neomax",
    "neomax-cli",
    "cmax",
    "cdx",
    "cdxmax",
    "ocx",
    "ocmax",
    "kmx",
    "kmax",
    "gmx",
    "gmax",
];
pub(crate) const AUXILIARIES: &[&str] =
    &["neomax-portal", "neomax-usage-agent", "neomax-worktrees"];
pub(crate) const SHELL_ASSETS: &[&str] = &["neomax-aliases.zsh", "neomax-shell-shortcuts.sh"];
pub(crate) const WORKFLOWS: &[&str] = &[
    "neomax.md",
    "rotate.md",
    "find-issues.md",
    "fix-issues.md",
    "project.md",
];

pub(crate) struct InstallFixture {
    _temp: TempDir,
    pub(crate) package: PathBuf,
    pub(crate) destination: PathBuf,
    pub(crate) home: PathBuf,
    workspace: PathBuf,
    pub(crate) provider_log: PathBuf,
}

impl InstallFixture {
    pub(crate) fn new() -> Self {
        let temp = tempfile::tempdir().expect("installation fixture directory");
        let package = temp.path().join("package");
        let destination = temp.path().join("destination");
        let home = temp.path().join("home");
        let workspace = temp.path().join("workspace");
        let provider_log = temp.path().join("provider-invocations.log");
        for path in [&package, &destination, &home, &workspace] {
            fs::create_dir_all(path).expect("installation fixture path");
        }
        Self {
            _temp: temp,
            package,
            destination,
            home,
            workspace,
            provider_log,
        }
    }

    pub(crate) fn materialize_package(&self) {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bin = self.package.join("bin");
        let share = self.package.join("share/neomax");
        let shell = share.join("shell");
        let workflows = share.join("workflows");
        let kimi_agents = share.join("agents");
        let docs = self.package.join("docs");
        for path in [&bin, &shell, &workflows, &kimi_agents, &docs] {
            fs::create_dir_all(path).expect("package fixture directory");
        }

        fs::copy(
            env!("CARGO_BIN_EXE_neomax"),
            bin.join(binary_name("neomax")),
        )
        .expect("package fixture neomax binary");
        for alias in ALIASES.iter().skip(1) {
            create_alias(&bin.join(binary_name(alias)));
        }
        for auxiliary in AUXILIARIES {
            let path = bin.join(binary_name(auxiliary));
            if *auxiliary == "neomax-usage-agent" && cfg!(unix) {
                fs::write(
                    &path,
                    b"#!/bin/sh\nprintf '%s\\n' 'usage service fixture'\nexit 0\n",
                )
                .expect("package fixture usage agent");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut permissions = fs::metadata(&path).unwrap().permissions();
                    permissions.set_mode(0o755);
                    fs::set_permissions(&path, permissions).unwrap();
                }
            } else {
                fs::write(&path, format!("fixture executable: {auxiliary}\n"))
                    .expect("package fixture auxiliary");
            }
        }

        copy_repo_file(&repo.join("LICENSE"), &self.package.join("LICENSE"));
        copy_repo_file(&repo.join("README.md"), &self.package.join("README.md"));
        copy_repo_file(
            &repo.join("crates/neomax-core/assets/opencode-model-policy.json"),
            &share.join("opencode-model-policy.json"),
        );
        for asset in SHELL_ASSETS {
            copy_repo_file(&repo.join("assets/shell").join(asset), &shell.join(asset));
        }
        copy_repo_file(
            &repo.join("docs/INSTALLATION.md"),
            &docs.join("INSTALLATION.md"),
        );
        for workflow in WORKFLOWS {
            copy_repo_file(
                &repo.join("assets/workflows").join(workflow),
                &workflows.join(workflow),
            );
        }
        copy_repo_file(
            &repo.join("assets/kimi/neomax-kimi.md"),
            &kimi_agents.join("neomax-kimi.md"),
        );
    }

    pub(crate) fn command(&self, executable: impl Into<PathBuf>) -> Command {
        let mut command = Command::new(executable.into());
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("NEOMAX_HOME", self.home.join(".neomax"))
            .env("NEOMAX_NO_USAGE_AGENT", "1")
            .env("NEOMAX_E2E_LOG", &self.provider_log)
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("ALL_PROXY", "http://127.0.0.1:9")
            .env("NO_PROXY", "")
            .env("GIT_CONFIG_GLOBAL", self.home.join("git-global"))
            .env("GIT_CONFIG_SYSTEM", self.home.join("git-system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", self.home.join("missing-git-askpass"))
            .env("SSH_ASKPASS", self.home.join("missing-ssh-askpass"))
            .env("GIT_EDITOR", "true")
            .env("GIT_SEQUENCE_EDITOR", "true")
            .env("GIT_PAGER", "cat")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_CONFIG_COUNT", "4")
            .env("GIT_CONFIG_KEY_0", "core.hooksPath")
            .env("GIT_CONFIG_VALUE_0", "__neomax_no_test_hooks__")
            .env("GIT_CONFIG_KEY_1", "commit.gpgSign")
            .env("GIT_CONFIG_VALUE_1", "false")
            .env("GIT_CONFIG_KEY_2", "tag.gpgSign")
            .env("GIT_CONFIG_VALUE_2", "false")
            .env("GIT_CONFIG_KEY_3", "core.fsmonitor")
            .env("GIT_CONFIG_VALUE_3", "false")
            .env("PATH", fixture_path());
        preserve_windows_command_shell_environment(&mut command);
        command
    }

    #[allow(dead_code, reason = "shared by installation integration targets")]
    pub(crate) fn run(&self, args: &[String]) -> Output {
        self.command(env!("CARGO_BIN_EXE_neomax"))
            .args(args)
            .output()
            .expect("run installation command")
    }

    #[cfg(unix)]
    #[allow(dead_code)]
    pub(crate) fn run_with_usage_agent(&self, args: &[String]) -> Output {
        self.command(env!("CARGO_BIN_EXE_neomax"))
            .env_remove("NEOMAX_NO_USAGE_AGENT")
            .args(args)
            .output()
            .expect("run installation command with usage agent")
    }

    pub(crate) fn paths_args(&self, operation: &str) -> Vec<String> {
        vec![
            operation.into(),
            "--json".into(),
            "--package-root".into(),
            self.package.to_string_lossy().into_owned(),
            "--install-root".into(),
            self.destination.to_string_lossy().into_owned(),
        ]
    }
}

fn preserve_windows_command_shell_environment(command: &mut Command) {
    #[cfg(windows)]
    for key in ["ComSpec", "SystemRoot"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn copy_repo_file(source: &Path, target: &Path) {
    fs::copy(source, target).unwrap_or_else(|error| {
        panic!(
            "copy package fixture file {} to {}: {error}",
            source.display(),
            target.display()
        )
    });
}

pub(crate) fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn fixture_path() -> std::ffi::OsString {
    #[cfg(windows)]
    {
        std::env::join_paths(windows_command_paths()).expect("fixture PATH entries are valid")
    }
    #[cfg(not(windows))]
    {
        std::env::join_paths([Path::new("/usr/bin"), Path::new("/bin")])
            .expect("fixture PATH entries are valid")
    }
}

#[cfg(windows)]
fn windows_command_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(command_shell) = std::env::var_os("ComSpec") {
        if let Some(parent) = PathBuf::from(command_shell).parent() {
            paths.push(parent.to_path_buf());
        }
    }
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        let system32 = PathBuf::from(system_root).join("System32");
        paths.push(system32.join("WindowsPowerShell/v1.0"));
        paths.push(system32);
    }
    paths.sort();
    paths.dedup();
    paths
}

fn create_alias(path: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink("neomax", path).expect("package fixture alias");
    #[cfg(windows)]
    fs::copy(
        path.parent()
            .expect("package fixture bin")
            .join(binary_name("neomax")),
        path,
    )
    .expect("package fixture alias");
}
