use std::collections::BTreeMap;

use serde_json::json;

use crate::orchestration::handoff::{AccountId, HandoffBaton};
use crate::runs::failover::FailoverTarget;
use crate::runs::{RunRecord, RunStatus};

use super::request::ContinuationRequest;
use super::service::{ContinuationMode, ContinuationOutcome};

pub fn build_baton(request: &ContinuationRequest) -> HandoffBaton {
    let mut extra = BTreeMap::new();
    extra.insert("run_id".into(), json!(request.run_id));
    extra.insert("source_profile".into(), json!(request.source_profile));
    extra.insert("target_profile".into(), json!(request.target.profile));
    extra.insert("workdir".into(), json!(request.workdir));
    extra.insert("repo".into(), json!(request.repo));
    extra.insert("worktree".into(), json!(request.worktree));
    extra.insert("branch".into(), json!(request.branch));
    extra.insert("base".into(), json!(request.base));
    extra.insert("project".into(), json!(request.project));
    extra.insert("session".into(), json!(request.session));
    extra.insert("prompt".into(), json!(request.prompt));
    extra.insert("attempt".into(), json!(request.attempt));
    extra.insert("trigger".into(), json!(request.trigger.as_str()));
    extra.insert("resets_at".into(), json!(request.resets_at));
    extra.insert("limit_window".into(), json!(request.limit_window));
    extra.insert("source_engine".into(), json!(request.engine));
    extra.insert("target_engine".into(), json!(request.target.engine));
    extra.insert("run_state".into(), json!(request.run_state));
    let five_hour = crate::accounts::engine_has_five_hour(request.engine)
        .then_some(request.five_hour)
        .flatten();
    HandoffBaton {
        ts: request.now.timestamp(),
        engine: request.engine,
        from_account: AccountId::from(request.source_account.clone()),
        to_account: Some(AccountId::from(request.target.account.clone())),
        reason: request.reason.clone(),
        cwd: request.cwd.clone(),
        five_hour,
        seven_day: request.seven_day,
        extra,
    }
}

pub fn apply_to_run(run: &mut RunRecord, target: &FailoverTarget, outcome: &ContinuationOutcome) {
    apply_to_run_with_resolver(
        run,
        target,
        outcome,
        &crate::runs::failover::NoModelOverrides,
    );
}

pub fn apply_to_run_with_resolver(
    run: &mut RunRecord,
    target: &FailoverTarget,
    outcome: &ContinuationOutcome,
    models: &dyn crate::runs::failover::ModelResolver,
) {
    if outcome.mode == ContinuationMode::CrossProviderHandoff {
        crate::runs::failover::apply_failover_with_resolver(run, target, models);
        return;
    }
    run.remember_session();
    if !run.tried.contains(&run.profile) {
        run.tried.push(run.profile.clone());
    }
    run.attempt = run.attempt.saturating_add(1);
    run.status = RunStatus::Running;
    run.ended = None;
    run.worker_pid = None;
    run.result_text = None;
    run.usage = None;
    run.resets_at = None;
    run.limit_window = None;
    run.error_detail = None;
    match outcome.mode {
        ContinuationMode::InPlaceAuthRotation => {
            if crate::providers::catalog::supports_native_resume(run.engine) {
                run.resume_session = outcome.resume_session.clone();
            } else {
                run.session = None;
                run.resume_session = None;
            }
            run.prompt_to_send = Some(format!(
                "{}{}",
                run.prompt,
                crate::runs::failover::SAME_PROVIDER_NOTE
            ));
            if !run.tried.contains(&target.account.profile) {
                run.tried.push(target.account.profile.clone());
            }
        }
        ContinuationMode::SameProviderHandoff => {
            run.engine = outcome.target_engine;
            run.profile = outcome.target_profile.clone();
            run.session = None;
            run.resume_session = None;
            run.prompt_to_send = Some(format!(
                "{}{}",
                run.prompt,
                crate::runs::failover::SAME_PROVIDER_NOTE
            ));
        }
        ContinuationMode::CrossProviderHandoff => unreachable!("cross-provider handled above"),
    }
}
