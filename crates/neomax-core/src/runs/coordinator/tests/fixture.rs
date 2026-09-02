use std::collections::VecDeque;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeZone, Utc};

use crate::orchestration::auth::{RotationEffects, RotationOperation};
use crate::orchestration::continuation::{CredentialRotationPort, HandoffPort};
use crate::orchestration::handoff::HandoffBaton;
use crate::providers::{AuthState, ParsedEvents, Provider, ProviderCommand, ProviderProfile};
use crate::runs::coordinator::{AttemptRunner, RunClock};
use crate::runs::{ProcessProbe, RunRecord, RunStatus};
use crate::usage::UsageCacheStore;
use crate::{Engine, Error, Result};

pub(super) struct ProviderFixture {
    pub engine: Engine,
    pub profiles: Vec<ProviderProfile>,
}

impl Provider for ProviderFixture {
    fn engine(&self) -> Engine {
        self.engine
    }

    fn binary(&self) -> &OsStr {
        OsStr::new("fixture")
    }

    fn default_model(&self) -> &str {
        "model"
    }

    fn profiles(&self) -> Result<Vec<ProviderProfile>> {
        Ok(self.profiles.clone())
    }

    fn auth_state(&self, _profile: &ProviderProfile) -> AuthState {
        AuthState::Authenticated
    }

    fn worker_command(
        &self,
        _context: &crate::providers::WorkerLaunchContext,
    ) -> Result<ProviderCommand> {
        Err(Error::Message("not used".into()))
    }

    fn parse_events(&self, _bytes: &[u8]) -> Result<ParsedEvents> {
        Ok(ParsedEvents::default())
    }
}

pub(super) fn profile(engine: Engine, account: &str, root: &Path) -> ProviderProfile {
    ProviderProfile {
        engine,
        account: account.into(),
        path: root.join(format!("{engine}-{account}")),
        reserved: false,
    }
}

pub(super) fn run(engine: Engine, profile: PathBuf, root: &Path) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id":"run", "engine":engine, "model":"user-model", "prompt":"complete work",
        "profile":profile, "workdir":root.join("workspace"), "status":"running", "started":1
    }))
    .unwrap()
}

pub(super) struct NoLiveWorkers;

impl ProcessProbe for NoLiveWorkers {
    fn pid_alive(&self, _pid: u32) -> bool {
        false
    }

    fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
        false
    }
}

pub(super) struct FixedClock;

impl RunClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(100, 0).unwrap()
    }
}

pub(super) struct AttemptSequence {
    statuses: Mutex<VecDeque<RunStatus>>,
    observed: Mutex<Vec<(Engine, PathBuf, PathBuf)>>,
}

impl AttemptSequence {
    pub fn new(statuses: impl IntoIterator<Item = RunStatus>) -> Self {
        Self {
            statuses: Mutex::new(statuses.into_iter().collect()),
            observed: Mutex::new(Vec::new()),
        }
    }

    pub fn observed(&self) -> Vec<(Engine, PathBuf, PathBuf)> {
        self.observed.lock().unwrap().clone()
    }
}

impl AttemptRunner for AttemptSequence {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus> {
        self.observed
            .lock()
            .unwrap()
            .push((run.engine, run.profile.clone(), run.workdir.clone()));
        let status = self
            .statuses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Error::Message("attempt sequence exhausted".into()))?;
        if status == RunStatus::Limit {
            run.resets_at = Some(500.0);
            run.limit_window = Some("weekly".into());
        }
        if status == RunStatus::Done {
            run.result_text = Some("complete".into());
        }
        Ok(status)
    }
}

pub(super) struct QuotaTransitionAttempt<'a> {
    usage: &'a UsageCacheStore,
    profile: PathBuf,
    statuses: Mutex<VecDeque<RunStatus>>,
}

impl<'a> QuotaTransitionAttempt<'a> {
    pub(super) fn new(usage: &'a UsageCacheStore, profile: PathBuf) -> Self {
        Self {
            usage,
            profile,
            statuses: Mutex::new(VecDeque::from([RunStatus::Limit, RunStatus::Done])),
        }
    }
}

impl AttemptRunner for QuotaTransitionAttempt<'_> {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus> {
        let status = self
            .statuses
            .lock()
            .unwrap()
            .pop_front()
            .expect("quota transition attempt sequence");
        if status == RunStatus::Limit {
            self.usage
                .save(
                    run.engine,
                    &self.profile,
                    &crate::usage::ProviderUsageCache {
                        five_hour: crate::usage::QuotaWindow {
                            used_percent: Some(99.0),
                            resets_at: Some(500.0),
                        },
                        ..Default::default()
                    },
                )
                .unwrap();
            run.resets_at = Some(500.0);
            run.limit_window = Some("5h".into());
        }
        Ok(status)
    }
}

pub(super) struct FixtureRotation {
    pub(super) calls: Arc<Mutex<Vec<(PathBuf, PathBuf)>>>,
}

impl CredentialRotationPort for FixtureRotation {
    fn supports(&self, engine: Engine) -> bool {
        matches!(engine, Engine::Claude | Engine::Codex)
    }

    fn swap(
        &self,
        engine: Engine,
        destination: &std::path::Path,
        source: &std::path::Path,
        _timestamp: i64,
        _reason: Option<String>,
    ) -> Result<RotationEffects> {
        self.calls
            .lock()
            .unwrap()
            .push((destination.to_path_buf(), source.to_path_buf()));
        Ok(RotationEffects {
            engine,
            operation: RotationOperation::Swap,
            destination: destination.to_path_buf(),
            source: Some(source.to_path_buf()),
            backup_paths: Vec::new(),
            invalidated_cache_paths: Vec::new(),
        })
    }
}

#[derive(Default)]
pub(super) struct FixtureHandoff {
    pub(super) batons: Mutex<Vec<HandoffBaton>>,
}

impl HandoffPort for FixtureHandoff {
    fn save(&self, baton: &HandoffBaton) -> Result<()> {
        self.batons.lock().unwrap().push(baton.clone());
        Ok(())
    }
}
