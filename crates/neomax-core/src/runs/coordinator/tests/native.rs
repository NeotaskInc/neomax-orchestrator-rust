use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;

use crate::providers::{
    AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile, ProviderRegistry,
};
use crate::runs::coordinator::{AttemptRunner, NativeAttemptRunner};
use crate::runs::{RunStatus, RunStore};
use crate::usage::UsageCacheStore;
use crate::{ConcurrencySettings, EffectiveSettings, Engine, Result, SettingsFile, StatePaths};

use super::fixture::run;

fn fixture_script(root: &std::path::Path, name: &str, body: &str) -> OsString {
    #[cfg(windows)]
    {
        let path = root.join(name);
        fs::write(&path, format!("@echo off\r\n{body}\r\n")).unwrap();
        path.into_os_string()
    }
    #[cfg(not(windows))]
    {
        let _ = (root, name, body);
        "/bin/sh".into()
    }
}

struct CommandProvider {
    binary: OsString,
}

impl Provider for CommandProvider {
    fn engine(&self) -> Engine {
        Engine::Codex
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        "fixture-model"
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        Ok(Vec::new())
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(
        &self,
        context: &crate::providers::WorkerLaunchContext,
    ) -> Result<ProviderCommand> {
        let request = context.request();
        let command = ProviderCommand::new(&self.binary, &request.cwd);
        #[cfg(windows)]
        {
            Ok(command)
        }
        #[cfg(not(windows))]
        {
            Ok(command.arg("-c").arg("printf fixture-ok"))
        }
    }

    fn parse_events(&self, bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(ParsedEvents {
            result_text: (String::from_utf8_lossy(bytes).trim() == "fixture-ok")
                .then(|| "complete".into()),
            ..ParsedEvents::default()
        })
    }
}

struct PlainLimitProvider {
    binary: OsString,
}

impl Provider for PlainLimitProvider {
    fn engine(&self) -> Engine {
        Engine::Codex
    }

    fn binary(&self) -> &OsStr {
        &self.binary
    }

    fn default_model(&self) -> &str {
        "fixture-model"
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        Ok(Vec::new())
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(
        &self,
        context: &crate::providers::WorkerLaunchContext,
    ) -> Result<ProviderCommand> {
        let command = ProviderCommand::new(&self.binary, &context.request().cwd);
        #[cfg(windows)]
        {
            Ok(command)
        }
        #[cfg(not(windows))]
        {
            Ok(command
                .arg("-c")
                .arg("printf 'usage limit reached' >&2; exit 1"))
        }
    }

    fn parse_events(&self, _bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }

    fn refresh_quota(
        &self,
        profile: &std::path::Path,
        session_id: Option<&str>,
        observed_at: f64,
    ) -> Result<Option<crate::providers::CodexQuotaRefreshResult>> {
        crate::providers::refresh_from_rollout(profile, session_id, observed_at)
    }
}

#[test]
fn native_runner_uses_the_adapter_supervisor_and_durable_spawn_path() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("workspace")).unwrap();
    let binary = fixture_script(temp.path(), "fixture.cmd", "echo(fixture-ok");
    let providers = ProviderRegistry::new([Box::new(CommandProvider {
        binary,
    }) as Box<dyn Provider>]);
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings::default(),
            ..SettingsFile::default()
        },
        temp.path().join("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    paths.ensure_runtime_dirs().unwrap();
    let runs = RunStore::new(&paths.runs);
    let usage = UsageCacheStore::new(&paths.usage);
    let mut item = run(
        Engine::Codex,
        temp.path().join("profiles/codex1"),
        temp.path(),
    );
    runs.create(&item).unwrap();
    let status = NativeAttemptRunner {
        providers: &providers,
        settings: &settings,
        paths: &paths,
        runs: &runs,
        quota: &usage,
    }
    .run_attempt(&mut item)
    .unwrap();
    assert_eq!(status, RunStatus::Done);
    assert_eq!(item.result_text.as_deref(), Some("complete"));
    assert!(runs.load("run").unwrap().worker_pid.is_some());
}

#[test]
fn native_runner_refreshes_codex_rollout_after_plain_text_limit() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("workspace")).unwrap();
    let profile = temp.path().join("profiles/codex1");
    let rollout_dir = profile.join("sessions/2026/08/23");
    fs::create_dir_all(&rollout_dir).unwrap();
    fs::write(
        rollout_dir.join("rollout-thread-1.jsonl"),
        include_bytes!("../../../../tests/fixtures/provider_events/codex-rate-limit.jsonl"),
    )
    .unwrap();
    let binary = fixture_script(
        temp.path(),
        "limit.cmd",
        "echo(usage limit reached 1>&2\r\nexit /b 1",
    );

    let providers = ProviderRegistry::new([Box::new(PlainLimitProvider {
        binary,
    }) as Box<dyn Provider>]);
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings::default(),
            ..SettingsFile::default()
        },
        temp.path().join("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap();
    let paths = StatePaths::new(temp.path(), temp.path().join("state"));
    paths.ensure_runtime_dirs().unwrap();
    let runs = RunStore::new(&paths.runs);
    let usage = UsageCacheStore::new(&paths.usage);
    let mut item = run(Engine::Codex, profile, temp.path());
    runs.create(&item).unwrap();

    let status = crate::runs::coordinator::NativeAttemptRunner {
        providers: &providers,
        settings: &settings,
        paths: &paths,
        runs: &runs,
        quota: &usage,
    }
    .run_attempt(&mut item)
    .unwrap();

    assert_eq!(status, RunStatus::Limit);
    assert_eq!(item.resets_at, Some(2_000.0));
    assert_eq!(item.limit_window.as_deref(), Some("weekly"));
}
