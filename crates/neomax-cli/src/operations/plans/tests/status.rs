use neomax_core::scheduler::persistence::{PlanRecord, PlanStatus};

use super::super::status::{plan_status, plan_statuses};
use super::fixtures::plan;

#[test]
fn plan_status_reports_all_parts_and_filters_by_id() {
    let fixture = tempfile::tempdir().unwrap();
    let paths = neomax_core::StatePaths::new(fixture.path(), fixture.path().join("state"));
    paths.ensure_runtime_dirs().unwrap();
    let persistence =
        neomax_core::scheduler::service::FilePlanPersistence::new(&paths.plans, &paths.events);
    let mut record = PlanRecord::new("batch-1", plan(), None, 10).unwrap();
    record.status = PlanStatus::Running;
    record
        .state
        .mark_running(
            "one",
            "batch-1-one",
            Some("neomax/batch-1-one".into()),
            Some("profile".into()),
            10.0,
        )
        .unwrap();
    neomax_core::scheduler::service::PersistencePort::create(&persistence, &record).unwrap();
    let selected = plan_status(&paths, "batch-1").unwrap();
    assert_eq!(selected.parts.len(), 1);
    assert_eq!(selected.parts[0].run_id.as_deref(), Some("batch-1-one"));
    assert_eq!(plan_statuses(&paths).unwrap().plans.len(), 1);
}
