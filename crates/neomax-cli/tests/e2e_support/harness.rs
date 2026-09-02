use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use neomax_core::agent_tools::{
    ManifestStore, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV, NEOMAX_TOOL_INSTRUCTION_ENV,
    NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV, NEOMAX_TOOL_POLICY_ENV,
    ORCHESTRATOR_TOOL_INSTRUCTION,
};
use neomax_core::usage::{ProviderUsageCache, QuotaWindow, UsageCacheStore};
use neomax_core::{Engine, StatePaths};
use tempfile::TempDir;

use super::fake_provider;
use super::invocation::{self, Invocation};
use super::profiles;

pub struct E2eHarness {
    pub(super) _temp: TempDir,
    pub(crate) home: PathBuf,
    pub(crate) state: PathBuf,
    pub(super) workspace: PathBuf,
    pub(super) log: PathBuf,
    pub(super) bin_dir: PathBuf,
    pub(super) poison_bin: PathBuf,
    pub(super) poison_log: PathBuf,
    pub(super) profiles: BTreeMap<Engine, Vec<PathBuf>>,
    pub(super) behavior: String,
}

impl E2eHarness {
    pub fn new(engines: impl IntoIterator<Item = Engine>) -> Self {
        Self::with_behavior(engines, "done")
    }

    #[allow(dead_code)]
    pub fn with_behavior(
        engines: impl IntoIterator<Item = Engine>,
        behavior: impl Into<String>,
    ) -> Self {
        let temp = tempfile::tempdir().expect("e2e temporary directory");
        let home = temp.path().join("home");
        let state = temp.path().join("state");
        let workspace = temp.path().join("workspace");
        let bin_dir = temp.path().join("fake-bin");
        let poison_bin = temp.path().join("poison-bin");
        let log = temp.path().join("provider-invocations.log");
        let poison_log = temp.path().join("poison-provider-invocations.log");
        for path in [&home, &state, &workspace, &bin_dir, &poison_bin] {
            fs::create_dir_all(path).expect("e2e directory");
        }
        initialize_repository(&workspace);
        fake_provider::write_fake_security(&bin_dir).expect("fake keychain command");

        // Keep every provider name shadowed in the fixture PATH. This makes a
        // missing explicit binary override fail into a fixture, never a host
        // installation.
        for engine in Engine::ALL {
            fake_provider::write_fake_provider(&bin_dir, engine).expect("fake provider");
            fake_provider::write_poison_provider(&poison_bin, engine).expect("poison provider");
        }

        let mut profiles_map = BTreeMap::new();
        for engine in engines {
            let profile = home.join(format!(".{}-acct1", profiles::profile_stem(engine)));
            profiles::seed_profile(engine, &profile);
            profiles_map.insert(engine, vec![profile]);
        }

        Self {
            _temp: temp,
            home,
            state,
            workspace,
            log,
            bin_dir,
            poison_bin,
            poison_log,
            profiles: profiles_map,
            behavior: behavior.into(),
        }
    }

    #[allow(dead_code)]
    pub fn add_profile(&mut self, engine: Engine, account: u32) -> PathBuf {
        let profile = self
            .home
            .join(format!(".{}-acct{account}", profiles::profile_stem(engine)));
        profiles::seed_profile(engine, &profile);
        self.profiles
            .entry(engine)
            .or_default()
            .push(profile.clone());
        profile
    }

    pub fn invocations(&self) -> Vec<Invocation> {
        invocation::parse(&fs::read_to_string(&self.log).unwrap_or_default())
    }

    #[allow(
        dead_code,
        reason = "shared by prompt-preservation integration targets"
    )]
    pub fn log_contents(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    #[allow(dead_code, reason = "shared by authorized agent integration targets")]
    pub fn authorized_orchestrator_environment(&self) -> Vec<(String, String)> {
        let manifest = self.state.join("agent-tools/manifest.json");
        ManifestStore::new(&manifest)
            .write_canonical()
            .expect("canonical agent manifest");
        vec![
            (NEOMAX_BIN_ENV.into(), env!("CARGO_BIN_EXE_neomax").into()),
            (
                NEOMAX_TOOL_MANIFEST_ENV.into(),
                manifest.to_string_lossy().into_owned(),
            ),
            (NEOMAX_TOOL_POLICY_ENV.into(), "orchestrator".into()),
            (NEOMAX_TOOL_DEPTH_ENV.into(), "0".into()),
            (NEOMAX_TOOL_MAX_DEPTH_ENV.into(), "4".into()),
            (
                NEOMAX_TOOL_INSTRUCTION_ENV.into(),
                ORCHESTRATOR_TOOL_INSTRUCTION.into(),
            ),
            ("NEOMAX_ROLE".into(), "opencode".into()),
            ("NEOMAX_ORCHESTRATOR".into(), "1".into()),
        ]
    }

    #[allow(dead_code, reason = "shared by lifecycle integration targets")]
    pub fn state_paths(&self) -> StatePaths {
        StatePaths::new(self.home.clone(), self.state.clone())
    }

    #[allow(dead_code, reason = "shared by worktree lifecycle integration targets")]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    #[allow(dead_code, reason = "shared by worktree lifecycle integration targets")]
    pub fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let fixture_root = self
            .workspace
            .parent()
            .expect("e2e workspace has a temporary parent")
            .join("git-fixture");
        let mut safe_args = vec![
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
        ];
        safe_args.extend_from_slice(args);
        let output = hermetic_git_command(cwd, &fixture_root)
            .args(safe_args)
            .output()
            .expect("git fixture command");
        assert!(
            output.status.success(),
            "git fixture command failed: git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[allow(dead_code, reason = "shared by provider integration targets")]
    pub fn assert_hermetic_invocations(&self) {
        let poison_log = fs::read_to_string(&self.poison_log).unwrap_or_default();
        assert!(
            poison_log.trim().is_empty(),
            "provider resolution reached the poison path:\n{poison_log}"
        );
        for invocation in self.invocations() {
            assert!(
                invocation.field("program").is_some_and(|program| {
                    fixture_program_is_under_bin(Path::new(program), &self.bin_dir)
                }),
                "provider invocation escaped the fixture bin directory: {:?}",
                invocation.field("program")
            );
            assert_eq!(
                invocation.field("network_proxy"),
                Some("http://127.0.0.1:9"),
                "provider invocation did not use the dead egress proxy"
            );
        }
    }

    #[allow(dead_code)]
    pub fn profile(&self, engine: Engine, account: usize) -> &Path {
        &self.profiles[&engine][account]
    }

    #[allow(dead_code)]
    pub fn seed_quota(&self, engine: Engine, profile: &Path, five_hour: f64) {
        let store = UsageCacheStore::new(self.state.join("usage"));
        store
            .save(
                engine,
                profile,
                &ProviderUsageCache {
                    five_hour: QuotaWindow {
                        used_percent: Some(five_hour),
                        resets_at: Some(4_000_000_000.0),
                    },
                    seven_day: QuotaWindow {
                        used_percent: Some(0.0),
                        resets_at: Some(4_000_000_000.0),
                    },
                    source: Some("hermetic-fixture".into()),
                    observed_at: Some(1_700_000_000.0),
                    ..ProviderUsageCache::default()
                },
            )
            .expect("quota fixture");
    }

    #[allow(dead_code)]
    pub fn run_path(&self, id: &str) -> PathBuf {
        self.state.join("runs").join(format!("{id}.json"))
    }
}

fn fixture_program_is_under_bin(program: &Path, bin_dir: &Path) -> bool {
    let Some(canonical_program) = fs::canonicalize(program).ok() else {
        return false;
    };
    let Some(canonical_bin) = fs::canonicalize(bin_dir).ok() else {
        return false;
    };
    canonical_program.starts_with(canonical_bin)
}

fn initialize_repository(workspace: &Path) {
    let fixture_root = workspace
        .parent()
        .expect("e2e workspace has a temporary parent")
        .join("git-fixture");
    let run = |args: &[&str]| {
        let mut safe_args = vec![
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
        ];
        safe_args.extend_from_slice(args);
        let output = hermetic_git_command(workspace, &fixture_root)
            .args(safe_args)
            .output()
            .expect("git fixture command");
        assert!(
            output.status.success(),
            "git fixture command failed: git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "fixture@example.invalid"]);
    run(&["config", "user.name", "Neomax Fixture"]);
    run(&["config", "commit.gpgSign", "true"]);
    run(&["config", "tag.gpgSign", "true"]);
    #[cfg(unix)]
    {
        let hook = workspace.join(".git/hooks/pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\nprintf hooked > \"$GIT_DIR/neomax-hook-fired\"\n",
        )
        .expect("fixture hook");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755))
            .expect("fixture hook permissions");
    }
    fs::write(workspace.join("fixture.txt"), "fixture\n").expect("fixture file");
    run(&["add", "fixture.txt"]);
    run(&["commit", "-qm", "fixture"]);
    assert!(
        !workspace.join(".git/neomax-hook-fired").exists(),
        "fixture hook executed despite the fail-closed hook path"
    );
}

fn hermetic_git_command(workspace: &Path, fixture_root: &Path) -> Command {
    let mut command = Command::new(fixture_git_binary());
    command
        .current_dir(workspace)
        .env_clear()
        .env("HOME", fixture_root.join("home"))
        .env("XDG_CONFIG_HOME", fixture_root.join("config"))
        .env("GIT_CONFIG_GLOBAL", fixture_root.join("global"))
        .env("GIT_CONFIG_SYSTEM", fixture_root.join("system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", fixture_root.join("missing-askpass"))
        .env("SSH_ASKPASS", fixture_root.join("missing-ssh-askpass"))
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_PAGER", "cat")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("PATH", fixture_path());
    command
}

fn fixture_path() -> OsString {
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
pub(super) fn fixture_git_binary() -> PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("git.exe"))
        .find(|candidate| candidate.is_file())
        .expect("host git executable is available for the fixture")
}

#[cfg(not(windows))]
fn fixture_git_binary() -> PathBuf {
    PathBuf::from("git")
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
