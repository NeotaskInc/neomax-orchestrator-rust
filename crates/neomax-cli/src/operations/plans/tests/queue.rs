use std::collections::BTreeMap;

use neomax_core::queue::UnknownSessions;

use super::super::queue::PlanQueueBridge;

#[test]
fn queue_bridge_uses_a_stable_scheduler_namespace_and_releases_leases() {
    let fixture = tempfile::tempdir().unwrap();
    let settings = neomax_core::EffectiveSettings::resolve(
        neomax_core::SettingsFile::default(),
        fixture.path().join("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap();
    let bridge = PlanQueueBridge::new(fixture.path().join("queue.json"), &settings);
    let lease = bridge
        .reserve("batch-1", 2, "pid-test", None, 10.0, &UnknownSessions)
        .unwrap();
    assert_eq!(lease.reservation.task, "scheduler:batch-1");
    assert_eq!(
        bridge
            .poll(&lease, 11.0, &UnknownSessions)
            .unwrap()
            .unwrap()
            .granted,
        2
    );
    assert_eq!(bridge.release(&lease, 12.0, &UnknownSessions).unwrap(), 1);
    assert!(
        bridge
            .snapshot(13.0, &UnknownSessions)
            .unwrap()
            .queue
            .is_empty()
    );
}
