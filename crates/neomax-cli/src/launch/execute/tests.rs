#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
use neomax_core::Engine;
#[cfg(unix)]
use neomax_core::orchestration::commands::Launcher;
#[cfg(unix)]
use neomax_core::orchestration::registry::OrchestratorStore;
#[cfg(unix)]
use neomax_core::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, ProviderRegistry,
    WorkerLaunchContext,
};

#[cfg(unix)]
use crate::launch::LaunchOptions;
#[cfg(unix)]
use crate::tests::fixture;

#[cfg(unix)]
use super::run_with_registry;

#[cfg(unix)]
struct FakeProvider {
    executable: PathBuf,
    profile: PathBuf,
    reserved: bool,
}

#[cfg(unix)]
impl Provider for FakeProvider {
    fn engine(&self) -> Engine {
        Engine::Claude
    }

    fn binary(&self) -> &OsStr {
        self.executable.as_os_str()
    }

    fn default_model(&self) -> &str {
        "fake-model"
    }

    fn profiles(&self) -> neomax_core::Result<Vec<ProviderProfile>> {
        Ok(vec![ProviderProfile {
            engine: Engine::Claude,
            account: if self.reserved { "orch" } else { "1" }.into(),
            path: self.profile.clone(),
            reserved: self.reserved,
        }])
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(
        &self,
        context: &WorkerLaunchContext,
    ) -> neomax_core::Result<ProviderCommand> {
        let mut command =
            ProviderCommand::new(self.executable.clone(), context.request().cwd.clone());
        for (key, value) in context.tools().variables() {
            command = command.env(key, value);
        }
        Ok(command)
    }

    fn parse_events(&self, _bytes: &[u8]) -> neomax_core::Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }
}

#[cfg(unix)]
#[test]
fn worker_launch_uses_a_fake_executable_and_persists_the_completed_run() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture();
    let repository = &fixture.context.cwd;
    let init = neomax_core::git::invoke(repository, ["init", "-q", "-b", "main"]).unwrap();
    assert!(init.success, "{}", init.stderr_text());
    for args in [
        ["config", "user.name", "Neomax Test"],
        ["config", "user.email", "test@example.invalid"],
    ] {
        let result = neomax_core::git::invoke(repository, args).unwrap();
        assert!(result.success, "{}", result.stderr_text());
    }
    fs::write(repository.join("base.txt"), "base\n").unwrap();
    for args in [&["add", "base.txt"][..], &["commit", "-qm", "base"][..]] {
        let result = neomax_core::git::invoke(repository, args).unwrap();
        assert!(result.success, "{}", result.stderr_text());
    }
    let executable = fixture.context.paths.state.join("fake-provider.sh");
    fs::write(
        &executable,
        "#!/bin/sh\ntest -n \"$NEOMAX_BIN\" && test -n \"$NEOMAX_TOOL_MANIFEST\" && test \"$NEOMAX_TOOL_POLICY\" = worker || exit 7\nprintf '%s\\n' '{\"type\":\"result\",\"subtype\":\"success\"}'\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let profile = fixture.context.paths.home.join(".claude-acct1");
    fs::create_dir_all(&profile).unwrap();
    let registry = ProviderRegistry::new([Box::new(FakeProvider {
        executable,
        profile,
        reserved: false,
    }) as Box<dyn Provider>]);
    let options = LaunchOptions::parse(
        Launcher::Universal,
        &[
            "--json".into(),
            "--foreground".into(),
            "auto".into(),
            "task".into(),
        ],
    )
    .unwrap();

    run_with_registry(
        Launcher::Universal,
        options,
        &fixture.context,
        true,
        &registry,
    )
    .unwrap();
    let runs = neomax_core::runs::RunStore::new(&fixture.context.paths.runs)
        .all()
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, neomax_core::runs::RunStatus::Done);
    assert!(runs[0].log.is_some());
}

#[cfg(unix)]
#[test]
fn root_orchestrator_uses_interactive_provider_without_a_worker_run_record() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = fixture();
    let executable = fixture.context.paths.state.join("fake-orchestrator.sh");
    let marker = fixture.context.paths.state.join("orchestrator.marker");
    let registry_directory = fixture.context.paths.orchestrators.display().to_string();
    let owner_pid = std::process::id();
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\ntest \"$NEOMAX_MODE\" = orchestrator || exit 7\ntest -z \"${{NEOMAX_WORKER-}}\" || exit 8\ntest \"$NEOMAX_CODEX_MODEL\" = fixture/codex-model || exit 9\ntest \"$NEOMAX_ORCH_PID\" = \"{owner_pid}\" || exit 10\ntest -n \"$(find '{registry_directory}' -maxdepth 1 -name '*.json' -print -quit)\" || exit 11\nprintf '%s\\n' root > '{marker}'\n",
            marker = marker.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let profile = fixture.context.paths.home.join(".claude-acct1");
    fs::create_dir_all(&profile).unwrap();
    let registry = ProviderRegistry::new([Box::new(FakeProvider {
        executable,
        profile,
        reserved: true,
    }) as Box<dyn Provider>]);
    let options = LaunchOptions::parse(
        Launcher::ProviderOrchestrator(Engine::Claude),
        &[
            "--json".into(),
            "--foreground".into(),
            "--codex-model".into(),
            "fixture/codex-model".into(),
            "--dedicated".into(),
            "task".into(),
        ],
    )
    .unwrap();

    run_with_registry(
        Launcher::ProviderOrchestrator(Engine::Claude),
        options,
        &fixture.context,
        true,
        &registry,
    )
    .unwrap();

    assert_eq!(fs::read_to_string(marker).unwrap().trim(), "root");
    let runs = neomax_core::runs::RunStore::new(&fixture.context.paths.runs)
        .all()
        .unwrap();
    assert!(
        runs.is_empty(),
        "root launch persisted worker runs: {runs:?}"
    );
    assert!(
        OrchestratorStore::new(&fixture.context.paths.orchestrators)
            .all(&neomax_core::runs::SystemProcessProbe, fixture.context.now)
            .unwrap()
            .is_empty()
    );
}
