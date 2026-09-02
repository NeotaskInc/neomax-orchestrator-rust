use std::time::Duration;

use super::*;
use crate::config::AgentConfig;
use neomax_core::config::StatePaths;

#[test]
fn plans_use_only_the_local_neomax_cli_and_expected_flags() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = AgentConfig::with_paths(crate::config::AgentPaths::for_state(
        StatePaths::new(temp.path(), temp.path().join(".neomax")),
    ));
    config.neomax_cli = "/tmp/neomax".into();
    config.maintenance_timeout = Duration::from_secs(7);
    let rotate = MaintenancePlan::for_action(&config, MaintenanceAction::RotateTick);
    let keepalive = MaintenancePlan::for_action(&config, MaintenanceAction::Keepalive);
    let tidy = MaintenancePlan::for_action(&config, MaintenanceAction::WorktreeTidy);
    assert_eq!(rotate.program, std::path::PathBuf::from("/tmp/neomax"));
    assert_eq!(rotate.args, ["rotate-tick", "--active"]);
    assert_eq!(keepalive.args, ["keepalive", "--once"]);
    assert_eq!(tidy.args, ["tidy", "--automatic", "--any", "--json"]);
    assert_eq!(rotate.timeout, Duration::from_secs(7));
    assert_eq!(keepalive.timeout, Duration::from_secs(7));
    assert_eq!(tidy.timeout, Duration::from_secs(300));
}

#[cfg(unix)]
#[test]
fn local_executor_reports_a_bounded_timeout() {
    let plan = MaintenancePlan {
        action: MaintenanceAction::RotateTick,
        program: "/bin/sh".into(),
        args: vec!["-c".into(), "sleep 1".into()],
        timeout: Duration::from_millis(10),
    };
    let result = LocalMaintenanceExecutor.execute(&plan).unwrap();
    assert!(result.timed_out);
    assert!(!result.succeeded);
}
