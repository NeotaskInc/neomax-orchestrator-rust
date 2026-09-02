use chrono::Utc;

use crate::accounts::{
    quota_advice, QuotaRotationAdvice, QuotaSnapshotSource, QuotaTarget, QuotaWindow,
};
use crate::providers::ProviderRegistry;
use crate::runs::execution::{
    apply_outcome, AttemptSupervisor, SupervisorConfig, SupervisorDirective,
};
use crate::runs::{RunRecord, RunStatus, RunStore};
use crate::{EffectiveSettings, Error, Result, StatePaths};

pub trait AttemptRunner: Send + Sync {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus>;
}

pub struct NativeAttemptRunner<'a> {
    pub providers: &'a ProviderRegistry,
    pub settings: &'a EffectiveSettings,
    pub paths: &'a StatePaths,
    pub runs: &'a RunStore,
    pub quota: &'a dyn QuotaSnapshotSource,
}

impl AttemptRunner for NativeAttemptRunner<'_> {
    fn run_attempt(&self, run: &mut RunRecord) -> Result<RunStatus> {
        let provider = self
            .providers
            .get(run.engine)
            .ok_or_else(|| Error::Provider {
                provider: run.engine.to_string(),
                message: "provider adapter is unavailable".into(),
            })?;
        let resume_session = crate::providers::catalog::supports_native_resume(run.engine)
            .then_some(run.resume_session.as_deref())
            .flatten();
        let prepared = crate::runs::execution::prepare_attempt_with_secret(
            provider,
            run,
            self.settings,
            self.paths,
            resume_session,
            self.providers
                .process_secret_for(&crate::providers::ProviderProfile {
                    engine: run.engine,
                    account: run.account(),
                    path: run.profile.clone(),
                    reserved: run
                        .extra
                        .get("orchestrator_reserved")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or_else(|| run.account().eq_ignore_ascii_case("orch")),
                }),
        )?;
        let target = QuotaTarget {
            engine: run.engine,
            profile: run.profile.clone(),
        };
        let run_id = run.id.clone();
        let resumed = resume_session.is_some();
        let mut outcome = AttemptSupervisor::new(provider, SupervisorConfig::for_run(run)?)
            .run_monitored(
                prepared,
                run,
                &self.paths.logs,
                resumed,
                |record| self.runs.save_preserving_kill(record).map(|_| ()),
                || match self.runs.load(&run_id) {
                    Ok(record) if record.killed => Ok(SupervisorDirective::Abort),
                    Ok(_) => Ok(supervisor_directive(quota_advice(
                        self.quota,
                        &target,
                        Utc::now(),
                    ))),
                    Err(error) => Err(error),
                },
            )?;
        if run.engine == crate::Engine::Codex
            && outcome.status == RunStatus::Limit
            && outcome.parsed.resets_at.is_none()
        {
            outcome.parsed.rate_limited = true;
            if crate::providers::codex_quota_refresh_request(&outcome.parsed).is_some() {
                if let Ok(Some(refresh)) = provider.refresh_quota(
                    &run.profile,
                    run.session.as_deref(),
                    Utc::now().timestamp_millis() as f64 / 1000.0,
                ) {
                    crate::providers::apply_codex_quota_refresh(&mut outcome.parsed, &refresh);
                    apply_outcome(run, &outcome, resumed);
                }
            }
        }
        run.resume_session = None;
        Ok(outcome.status)
    }
}

fn supervisor_directive(advice: QuotaRotationAdvice) -> SupervisorDirective {
    if !advice.rotate {
        return SupervisorDirective::Continue;
    }
    SupervisorDirective::Rotate(crate::runs::execution::QuotaRotation {
        reason: advice.reason,
        resets_at: advice.resets_at.map(|value| value.timestamp() as f64),
        limit_window: advice
            .limit_window
            .map(QuotaWindow::as_str)
            .map(str::to_owned),
    })
}
