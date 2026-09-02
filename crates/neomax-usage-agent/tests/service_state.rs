use std::fs;
use std::sync::Arc;

use neomax_usage_agent::{
    AgentConfig, AgentPaths, MaintenanceExecutor, MaintenancePlan, MaintenanceResult,
    QuotaRefresher, QuotaReport, RunOptions, WatchService, WatchState,
};

mod support;

#[derive(Default)]
struct HermeticQuota;

impl QuotaRefresher for HermeticQuota {
    fn refresh(&self, _force: bool) -> anyhow::Result<QuotaReport> {
        Ok(QuotaReport::default())
    }
}

#[derive(Default)]
struct HermeticMaintenance;

impl MaintenanceExecutor for HermeticMaintenance {
    fn execute(&self, plan: &MaintenancePlan) -> anyhow::Result<MaintenanceResult> {
        Ok(MaintenanceResult {
            action: plan.action,
            exit_code: Some(0),
            timed_out: false,
            succeeded: true,
        })
    }
}

fn hermetic_service(paths: AgentPaths) -> WatchService {
    let collector = neomax_usage_agent::UsageCollector::with_now(paths.clone(), 1_800_000_000);
    WatchService::with_components(
        AgentConfig::with_paths(paths),
        collector,
        Arc::new(HermeticQuota),
        Arc::new(HermeticMaintenance),
    )
}

#[test]
fn first_once_backfills_and_second_once_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let transcript = paths
        .home
        .join(".claude")
        .join("projects")
        .join("demo")
        .join("session.jsonl");
    fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    fs::write(
        &transcript,
        r#"{"timestamp":"2026-05-30T12:00:00Z","sessionId":"s","message":{"role":"assistant","id":"m1","model":"claude-fable-5","usage":{"input_tokens":2,"output_tokens":3}}}
"#,
    )
    .unwrap();
    let service = hermetic_service(paths.clone());
    let first = service.run_once(RunOptions::default()).unwrap();
    assert_eq!(first.bootstrap.unwrap().records_emitted, 1);
    assert!(first.baselined);
    let second = service.run_once(RunOptions::default()).unwrap();
    assert_eq!(second.sweep.records_emitted, 0);
    assert!(
        WatchState::load(&paths.state.usage_watch)
            .unwrap()
            .baselined
    );
}

#[test]
fn malformed_watch_state_fails_closed_without_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    fs::create_dir_all(paths.state.state.clone()).unwrap();
    fs::write(&paths.state.usage_watch, "{not-json").unwrap();
    let error = WatchState::load(&paths.state.usage_watch).unwrap_err();
    assert!(error.to_string().contains("decode watch state"));
    assert_eq!(
        fs::read_to_string(paths.state.usage_watch).unwrap(),
        "{not-json"
    );
}

#[test]
fn older_watch_state_loads_without_worktree_tidy_fields() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    fs::create_dir_all(paths.state.state.clone()).unwrap();
    fs::write(
        &paths.state.usage_watch,
        r#"{"baselined":true,"maintenance":{"last_rotation_attempt":12,"last_keepalive_attempt":13,"last_rotation":null,"last_keepalive":null},"future":42}"#,
    )
    .unwrap();

    let state = WatchState::load(&paths.state.usage_watch).unwrap();
    assert!(state.baselined);
    assert_eq!(state.maintenance.last_rotation_attempt, Some(12));
    assert!(state.maintenance.last_worktree_tidy_attempt.is_none());
    assert!(state.maintenance.last_worktree_tidy.is_none());
    assert_eq!(state.extra["future"], serde_json::json!(42));
}

#[test]
fn no_backfill_seeds_offsets_without_importing_existing_history() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let transcript = paths
        .home
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("session.jsonl");
    fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    fs::write(
        &transcript,
        r#"{"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":4}}}}
"#,
    )
    .unwrap();
    let service = hermetic_service(paths.clone());
    let report = service
        .run_once(RunOptions {
            no_backfill: true,
            once: true,
            ..RunOptions::default()
        })
        .unwrap();
    assert_eq!(report.bootstrap.unwrap().records_emitted, 0);
    assert!(report.baselined);
    let ledger = neomax_core::usage::UsageLedger::new(paths.state.usage_ledger);
    assert!(
        ledger
            .read_deduplicated(0, 1_800_000_000)
            .unwrap()
            .is_empty()
    );
}
