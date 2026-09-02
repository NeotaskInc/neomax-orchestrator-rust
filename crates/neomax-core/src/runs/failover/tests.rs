use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;

use crate::accounts::{AccountSnapshot, SelectionPolicy};
use crate::runs::{RunRecord, RunStatus};
use crate::{Engine, WorkerScope};

use super::*;

fn account(engine: Engine, name: &str, weekly: f64) -> AccountSnapshot {
    AccountSnapshot {
        engine,
        account: name.into(),
        profile: PathBuf::from(format!("/profiles/{engine}-{name}")),
        binary_available: true,
        authenticated: true,
        rotation_eligible: true,
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: Some(0.0),
        weekly_percent: Some(weekly),
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    }
}

fn run(engine: Engine, profile: &PathBuf) -> RunRecord {
    serde_json::from_value(serde_json::json!({
        "id":"run", "engine":engine, "model":"user-model", "prompt":"finish work",
        "profile":profile, "workdir":"/workspace", "repo":"/repo", "branch":"work",
        "status":"running", "started":1, "attempt":1, "session":"session-1",
        "effort":"max", "ultra":true, "resets_at":500, "limit_window":"weekly",
        "usage":{"output":10}, "result_text":"partial", "error_detail":"old"
    }))
    .unwrap()
}

#[test]
fn keeps_the_reference_cross_provider_order_inside_scope() {
    let scope: WorkerScope = "opencode,kimi,claude".parse().unwrap();
    assert_eq!(
        cross_provider_order(Engine::Codex, &scope),
        [Engine::Opencode, Engine::Kimi, Engine::Claude]
    );
}

#[test]
fn prefers_another_account_before_crossing_provider() {
    let current = account(Engine::Claude, "1", 10.0);
    let next = account(Engine::Claude, "2", 20.0);
    let free = account(Engine::Opencode, "1", 0.0);
    let run = run(Engine::Claude, &current.profile);
    let decision = plan_failover(
        &run,
        RunStatus::Limit,
        &[current, next.clone(), free],
        &WorkerScope::all(),
        Utc::now(),
        &SelectionPolicy::default(),
    );
    let FailoverDecision::Continue(target) = decision else {
        panic!("expected failover");
    };
    assert_eq!(target.account.profile, next.profile);
    assert!(!target.crosses_provider);
}

#[test]
fn crosses_provider_for_limits_but_not_generic_errors() {
    let current = account(Engine::Claude, "1", 99.0);
    let next = account(Engine::Opencode, "1", 0.0);
    let run = run(Engine::Claude, &current.profile);
    let accounts = [current, next.clone()];
    let limit = plan_failover(
        &run,
        RunStatus::Limit,
        &accounts,
        &WorkerScope::all(),
        Utc::now(),
        &SelectionPolicy::default(),
    );
    let FailoverDecision::Continue(target) = limit else {
        panic!("expected cross-provider failover");
    };
    assert_eq!(target.account.profile, next.profile);
    assert!(target.crosses_provider);
    assert_eq!(
        plan_failover(
            &run,
            RunStatus::Error,
            &accounts,
            &WorkerScope::all(),
            Utc::now(),
            &SelectionPolicy::default(),
        ),
        FailoverDecision::Stop(FailoverStop::NoEligibleAccount)
    );
}

#[test]
fn failover_skips_a_connected_provider_until_its_binary_returns() {
    let source = account(Engine::Claude, "1", 99.0);
    let mut unavailable = account(Engine::Opencode, "1", 0.0);
    unavailable.binary_available = false;
    let available = account(Engine::Kimi, "1", 0.0);
    let run = run(Engine::Claude, &source.profile);
    let accounts = [source, unavailable, available.clone()];
    let decision = plan_failover(
        &run,
        RunStatus::Limit,
        &accounts,
        &WorkerScope::all(),
        Utc::now(),
        &SelectionPolicy::default(),
    );
    let FailoverDecision::Continue(target) = decision else {
        panic!("expected failover to an available provider");
    };
    assert_eq!(target.account.profile, available.profile);
    assert!(target.crosses_provider);
}

#[test]
fn respects_failover_guards_and_attempt_capacity() {
    let first = account(Engine::Codex, "1", 0.0);
    let second = account(Engine::Codex, "2", 0.0);
    let accounts = [first.clone(), second];
    let mut item = run(Engine::Codex, &first.profile);
    item.no_failover = true;
    assert_eq!(
        plan_failover(
            &item,
            RunStatus::Limit,
            &accounts,
            &WorkerScope::all(),
            Utc::now(),
            &SelectionPolicy::default(),
        ),
        FailoverDecision::Stop(FailoverStop::Disabled)
    );
    item.no_failover = false;
    item.resumed = true;
    assert_eq!(
        plan_failover(
            &item,
            RunStatus::Limit,
            &accounts,
            &WorkerScope::all(),
            Utc::now(),
            &SelectionPolicy::default(),
        ),
        FailoverDecision::Stop(FailoverStop::ResumedRun)
    );
    item.resumed = false;
    item.attempt = 1;
    let FailoverDecision::Continue(target) = plan_failover(
        &item,
        RunStatus::Limit,
        &accounts,
        &WorkerScope::all(),
        Utc::now(),
        &SelectionPolicy::default(),
    ) else {
        panic!("a fresh Codex attempt must remain eligible for quota failover");
    };
    assert_eq!(target.account.profile, accounts[1].profile);
    item.attempt = 2;
    assert_eq!(
        plan_failover(
            &item,
            RunStatus::Limit,
            &accounts,
            &WorkerScope::all(),
            Utc::now(),
            &SelectionPolicy::default(),
        ),
        FailoverDecision::Stop(FailoverStop::AttemptsExhausted)
    );
}

#[test]
fn transitions_in_place_without_losing_worktree_or_session_history() {
    let source = account(Engine::Claude, "1", 99.0);
    let target = account(Engine::Codex, "2", 0.0);
    let mut item = run(Engine::Claude, &source.profile);
    apply_failover(
        &mut item,
        &FailoverTarget {
            account: target.clone(),
            crosses_provider: true,
        },
    );
    assert_eq!(item.engine, Engine::Codex);
    assert_eq!(item.profile, target.profile);
    assert_eq!(item.model, "gpt-5.6-sol");
    assert_eq!(item.effort.as_deref(), Some("xhigh"));
    assert_eq!(item.workdir, PathBuf::from("/workspace"));
    assert_eq!(item.branch.as_deref(), Some("work"));
    assert_eq!(item.attempt, 2);
    assert_eq!(item.session, None);
    assert_eq!(item.session_history[0].session, "session-1");
    assert!(item.prompt_for_attempt().contains(CROSS_PROVIDER_NOTE));
    assert!(item.resets_at.is_none());
    assert!(item.usage.is_none());
    assert!(item.result_text.is_none());
}

#[test]
fn same_provider_transition_preserves_model_and_renews_claude_session() {
    let source = account(Engine::Claude, "1", 0.0);
    let target = account(Engine::Claude, "2", 0.0);
    let mut item = run(Engine::Claude, &source.profile);
    apply_failover(
        &mut item,
        &FailoverTarget {
            account: target,
            crosses_provider: false,
        },
    );
    assert_eq!(item.model, "user-model");
    assert!(
        item.session
            .as_deref()
            .is_some_and(|value| value != "session-1")
    );
    assert!(item.prompt_for_attempt().contains(SAME_PROVIDER_NOTE));
}

#[test]
fn cross_provider_transition_uses_the_explicit_target_provider_model() {
    let source = account(Engine::Claude, "1", 99.0);
    let target = account(Engine::Codex, "2", 0.0);
    let mut item = run(Engine::Claude, &source.profile);
    let models = BTreeMap::from([(Engine::Codex, "gpt-5.6-terra".to_string())]);
    apply_failover_with_resolver(
        &mut item,
        &FailoverTarget {
            account: target,
            crosses_provider: true,
        },
        &models,
    );
    assert_eq!(item.engine, Engine::Codex);
    assert_eq!(item.model, "gpt-5.6-terra");
}

#[test]
fn cross_provider_transition_keeps_an_arbitrary_local_target_model() {
    let source = account(Engine::Claude, "1", 99.0);
    let target = account(Engine::Opencode, "2", 0.0);
    let mut item = run(Engine::Claude, &source.profile);
    let models = BTreeMap::from([(Engine::Opencode, "local/custom/model:latest".to_string())]);
    apply_failover_with_resolver(
        &mut item,
        &FailoverTarget {
            account: target,
            crosses_provider: true,
        },
        &models,
    );
    assert_eq!(item.engine, Engine::Opencode);
    assert_eq!(item.model, "local/custom/model:latest");
}

#[test]
fn same_provider_transition_does_not_replace_the_current_model_with_target_override() {
    let source = account(Engine::Codex, "1", 99.0);
    let target = account(Engine::Codex, "2", 0.0);
    let mut item = run(Engine::Codex, &source.profile);
    let models = BTreeMap::from([(Engine::Codex, "gpt-5.6-terra".to_string())]);
    apply_failover_with_resolver(
        &mut item,
        &FailoverTarget {
            account: target,
            crosses_provider: false,
        },
        &models,
    );
    assert_eq!(item.model, "user-model");
}
