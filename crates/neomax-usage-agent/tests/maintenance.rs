use std::sync::{Arc, Mutex};

use neomax_usage_agent::{
    AgentConfig, MaintenanceAction, MaintenanceExecutor, MaintenancePlan, MaintenanceResult,
    QuotaRefresher, QuotaReport, SweepMode, UsageCollector, WatchService, WatchState,
};

mod support;

#[derive(Default)]
struct FakeMaintenance {
    plans: Mutex<Vec<MaintenancePlan>>,
}

impl MaintenanceExecutor for FakeMaintenance {
    fn execute(&self, plan: &MaintenancePlan) -> anyhow::Result<MaintenanceResult> {
        self.plans.lock().unwrap().push(plan.clone());
        Ok(MaintenanceResult {
            action: plan.action,
            exit_code: Some(0),
            timed_out: false,
            succeeded: true,
        })
    }
}

#[derive(Default)]
struct FakeQuota {
    forces: Mutex<Vec<bool>>,
}

impl QuotaRefresher for FakeQuota {
    fn refresh(&self, force: bool) -> anyhow::Result<QuotaReport> {
        self.forces.lock().unwrap().push(force);
        Ok(QuotaReport::default())
    }
}

#[test]
fn first_cycle_runs_rotation_and_keepalive_through_the_injected_executor() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_000);
    let config = AgentConfig::with_paths(paths.clone());
    let fake = Arc::new(FakeMaintenance::default());
    let service = WatchService::with_maintenance(config, collector, fake.clone());
    let report = service.run_once(Default::default()).unwrap();
    assert_eq!(report.maintenance.len(), 3);
    assert_eq!(report.maintenance[0].action, MaintenanceAction::RotateTick);
    assert_eq!(report.maintenance[1].action, MaintenanceAction::Keepalive);
    assert_eq!(
        report.maintenance[2].action,
        MaintenanceAction::WorktreeTidy
    );
    assert!(report.maintenance.iter().all(|item| item.succeeded));
    let plans = fake.plans.lock().unwrap();
    assert_eq!(plans[0].args, ["rotate-tick", "--active"]);
    assert_eq!(plans[1].args, ["keepalive", "--once"]);
    assert_eq!(plans[2].args, ["tidy", "--automatic", "--any", "--json"]);
    drop(plans);

    let second = service.run_once(Default::default()).unwrap();
    assert!(second.maintenance.is_empty());
    let state = WatchState::load(&paths.state.usage_watch).unwrap();
    assert!(state.maintenance.last_rotation.unwrap().succeeded);
    assert!(state.maintenance.last_keepalive.unwrap().succeeded);
    assert!(state.maintenance.last_worktree_tidy.unwrap().succeeded);
}

#[test]
fn worktree_tidy_can_be_disabled_without_affecting_other_maintenance() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_000);
    let mut config = AgentConfig::with_paths(paths.clone());
    config.worktree_tidy_interval = None;
    let fake = Arc::new(FakeMaintenance::default());
    let service = WatchService::with_maintenance(config, collector, fake.clone());

    let report = service.run_once(Default::default()).unwrap();
    assert_eq!(report.maintenance.len(), 2);
    assert!(
        report
            .maintenance
            .iter()
            .all(|report| report.action != MaintenanceAction::WorktreeTidy)
    );
    assert!(
        WatchState::load(&paths.state.usage_watch)
            .unwrap()
            .maintenance
            .last_worktree_tidy_attempt
            .is_none()
    );
}

#[test]
fn collection_and_maintenance_are_separate_injection_seams() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_000);
    let mut state = WatchState::default();
    let sweep = collector.sweep(&mut state, SweepMode::Baseline, 0).unwrap();
    assert_eq!(sweep.records_emitted, 0);
    assert!(!state.baselined);
}

#[test]
fn rate_limited_collection_forces_quota_refresh_before_maintenance() {
    let temp = tempfile::tempdir().unwrap();
    let paths = support::agent_paths(&temp);
    let rollout = paths
        .home
        .join(".codex")
        .join("sessions")
        .join("2026")
        .join("limited.jsonl");
    std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    std::fs::write(
        rollout,
        r#"{"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":4}},"rate_limits":{"primary":{"used_percent":99}}}}
"#,
    )
    .unwrap();
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_000);
    let quota = Arc::new(FakeQuota::default());
    let maintenance = Arc::new(FakeMaintenance::default());
    let service = WatchService::with_components(
        AgentConfig::with_paths(paths),
        collector,
        quota.clone(),
        maintenance,
    );

    let report = service.run_once(Default::default()).unwrap();
    assert_eq!(report.quota, QuotaReport::default());
    assert_eq!(*quota.forces.lock().unwrap(), vec![true]);
}
