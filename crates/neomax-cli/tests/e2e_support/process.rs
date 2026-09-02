use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};

use neomax_core::Engine;
use neomax_core::io::process_group;

use super::fake_provider;
use super::harness::E2eHarness;
use super::profiles;

pub struct CommandResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub struct E2eChild {
    child: Child,
    diagnostics: PathBuf,
}

impl E2eChild {
    fn new(child: Child, diagnostics: PathBuf) -> Self {
        Self { child, diagnostics }
    }

    #[allow(dead_code, reason = "available to process-isolation fixtures")]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait()
    }

    pub fn diagnostics(&self) -> String {
        let bytes = std::fs::read(&self.diagnostics).unwrap_or_default();
        let start = bytes.len().saturating_sub(8_000);
        String::from_utf8_lossy(&bytes[start..]).into_owned()
    }

    pub(super) fn as_child_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl Drop for E2eChild {
    fn drop(&mut self) {
        let _ = process_group::terminate_detached(&mut self.child);
    }
}

impl CommandResult {
    #[allow(
        dead_code,
        reason = "shared by integration targets with JSON assertions"
    )]
    pub fn assert_success(&self) {
        assert!(
            self.status.success(),
            "command failed\nstdout:\n{}\nstderr:\n{}",
            self.stdout,
            self.stderr
        );
    }

    #[allow(
        dead_code,
        reason = "shared by integration targets with JSON assertions"
    )]
    pub fn json(&self) -> serde_json::Value {
        self.assert_success();
        serde_json::from_str(self.stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "command did not produce JSON: {error}\nstdout:\n{}\nstderr:\n{}",
                self.stdout, self.stderr
            )
        })
    }
}

impl E2eHarness {
    #[allow(dead_code)]
    pub fn run<I, S>(&self, args: I) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command("neomax");
        command.args(args);
        command.output_result()
    }

    #[allow(dead_code)]
    pub fn run_with_env<I, S, K, V>(
        &self,
        args: I,
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut command = self.command("neomax");
        command.args(args).envs(environment);
        command.output_result()
    }

    #[allow(dead_code)]
    pub fn run_without_binary_override<I, S>(&self, engine: Engine, args: I) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command("neomax");
        command.env_remove(profiles::binary_env(engine)).args(args);
        command.output_result()
    }

    #[allow(dead_code)]
    pub fn run_alias<I, S>(&self, alias: &str, args: I) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(alias);
        command.args(args);
        command.output_result()
    }

    #[allow(dead_code)]
    pub fn run_alias_with_env<I, S, K, V>(
        &self,
        alias: &str,
        args: I,
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut command = self.command(alias);
        command.args(args).envs(environment);
        command.output_result()
    }

    #[allow(dead_code)]
    pub fn run_alias_with_stdin<I, S>(&self, alias: &str, args: I, input: &[u8]) -> CommandResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command(alias);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn e2e command");
        child
            .stdin
            .take()
            .expect("e2e command stdin pipe")
            .write_all(input)
            .expect("write e2e command stdin");
        let output = child.wait_with_output().expect("wait e2e command");
        CommandResult {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    #[allow(dead_code)]
    pub fn spawn<I, S>(&self, args: I) -> E2eChild
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.command("neomax");
        command.args(args);
        self.spawn_command(command)
    }

    #[allow(dead_code)]
    pub fn spawn_with_env<I, S, K, V>(
        &self,
        args: I,
        environment: impl IntoIterator<Item = (K, V)>,
    ) -> E2eChild
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut command = self.command("neomax");
        command.args(args).envs(environment);
        self.spawn_command(command)
    }

    fn spawn_command(&self, mut command: Command) -> E2eChild {
        let diagnostics = self.state.join("spawned-neomax.stderr.log");
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&diagnostics)
            .expect("open neomax diagnostics");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr));
        let child = process_group::spawn_detached(&mut command).expect("spawn neomax");
        E2eChild::new(child, diagnostics)
    }

    fn command(&self, invocation: &str) -> Command {
        let mut command = Command::new(self.binary_for(invocation));
        command
            .current_dir(&self.workspace)
            .env_clear()
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("NEOMAX_HOME", &self.state)
            .env("NEOMAX_E2E_LOG", &self.log)
            .env("NEOMAX_E2E_POISON_LOG", &self.poison_log)
            .env("NEOMAX_E2E_BEHAVIOR", &self.behavior)
            .env("NEOMAX_MAX_SUBAGENTS", "17")
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
            .env("PATH", self.fixture_path());
        preserve_windows_command_shell_environment(&mut command);
        for engine in Engine::ALL {
            command.env(profiles::binary_env(engine), self.fake_binary(engine));
            if let Some(profiles) = self.profiles.get(&engine) {
                let joined = std::env::join_paths(profiles)
                    .expect("profile paths are valid for the e2e fixture");
                command.env(profiles::profile_env(engine), joined);
            }
        }
        command
    }

    fn binary_for(&self, invocation: &str) -> PathBuf {
        if invocation == "neomax" {
            return PathBuf::from(env!("CARGO_BIN_EXE_neomax"));
        }
        let path = fake_provider::alias_path(&self.bin_dir, invocation);
        if !path.exists() {
            fake_provider::create_alias(&path).expect("launcher alias");
        }
        path
    }

    fn fake_binary(&self, engine: Engine) -> PathBuf {
        self.bin_dir.join(fake_provider::fake_name(engine))
    }

    fn fixture_path(&self) -> OsString {
        let mut paths = vec![self.bin_dir.clone(), self.poison_bin.clone()];
        #[cfg(windows)]
        paths.extend(windows_command_paths());
        #[cfg(not(windows))]
        paths.extend([PathBuf::from("/usr/bin"), PathBuf::from("/bin")]);
        std::env::join_paths(paths).expect("fixture PATH entries are valid")
    }
}

#[cfg(windows)]
fn windows_command_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(parent) = super::harness::fixture_git_binary().parent() {
        paths.push(parent.to_path_buf());
    }
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

#[cfg(windows)]
#[test]
fn shell_environment_retains_only_windows_command_prerequisites() {
    use std::collections::BTreeMap;

    let mut command = Command::new("fixture");
    command.env_clear();
    preserve_windows_command_shell_environment(&mut command);
    let environment = command
        .get_envs()
        .map(|(key, value)| (key.to_os_string(), value.map(OsStr::to_os_string)))
        .collect::<BTreeMap<_, _>>();

    for key in ["ComSpec", "SystemRoot"] {
        let expected = std::env::var_os(key).map(Some);
        assert_eq!(environment.get(OsStr::new(key)), expected.as_ref());
    }
    let expected_count = ["ComSpec", "SystemRoot"]
        .into_iter()
        .filter(|key| std::env::var_os(key).is_some())
        .count();
    assert_eq!(environment.len(), expected_count);
}

trait CommandOutputExt {
    fn output_result(self) -> CommandResult;
}

impl CommandOutputExt for Command {
    fn output_result(mut self) -> CommandResult {
        let output = self.output().expect("run e2e command");
        CommandResult {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}
