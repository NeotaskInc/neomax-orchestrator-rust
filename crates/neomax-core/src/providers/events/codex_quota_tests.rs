use serde_json::json;

use super::rollout::CODEX_ROLLOUT_TAIL_BYTES;
use super::*;
use crate::providers::ParsedEvents;

#[test]
fn bounds_injected_refresh_request_without_network_behavior() {
    let request = CodexQuotaRefreshRequest::bounded(60_000);
    assert_eq!(request.method, CODEX_RATE_LIMIT_REFRESH_METHOD);
    assert_eq!(request.timeout_ms, CODEX_RATE_LIMIT_REFRESH_TIMEOUT_MS);
    assert_eq!(CodexQuotaRefreshRequest::bounded(1).timeout_ms, 100);
}

#[test]
fn parses_nested_reset_metadata_and_applies_it() {
    let result = CodexQuotaRefreshResult::from_value(
        json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": {"usedPercent": 100, "windowDurationMins": 300, "resetsAt": 2_000},
                "secondary": {"usedPercent": 50, "windowDurationMins": 10_080, "resetsAt": 4_000}
            }
        }),
        1_000.0,
    )
    .unwrap();
    assert_eq!(result.resets_at, Some(2_000.0));
    assert_eq!(result.limit_window.as_deref(), Some("five_hour"));
    let mut parsed = ParsedEvents {
        rate_limited: true,
        ..ParsedEvents::default()
    };
    result.apply_to(&mut parsed);
    assert_eq!(parsed.resets_at, Some(2_000.0));
    assert_eq!(parsed.limit_window.as_deref(), Some("five_hour"));
}

#[test]
fn accepts_rollout_snake_case_windows() {
    let result = CodexQuotaRefreshResult::from_value(
        json!({"rate_limits":{"primary":{"used_percent":100,"window_minutes":10080,"resets_at":2_000},"secondary":null}}),
        1_000.0,
    )
    .unwrap();
    assert_eq!(result.limit_window.as_deref(), Some("weekly"));
    assert_eq!(result.resets_at, Some(2_000.0));
}

#[test]
fn treats_hard_wall_and_provider_reached_type_as_blocking() {
    let result = CodexQuotaRefreshResult::from_value(
        json!({
            "rateLimits": {
                "rateLimitReachedType": "rate_limit_reached",
                "primary": {"usedPercent": 99, "windowDurationMins": 300, "resetsAt": 2_000}
            }
        }),
        1_000.0,
    )
    .unwrap();
    assert!(result.blocks_new_work());
    assert_eq!(
        result.rate_limit_reached_type.as_deref(),
        Some("rate_limit_reached")
    );
}

#[test]
fn finds_limit_snapshot_in_by_limit_id_response() {
    let result = CodexQuotaRefreshResult::from_value(
        json!({
            "rateLimitsByLimitId": {
                "other": {"primary": {"usedPercent": 20, "windowDurationMins": 30, "resetsAt": 2_000}},
                "codex": {"primary": {"usedPercent": 99, "windowDurationMins": 300, "resetsAt": 3_000}}
            }
        }),
        1_000.0,
    )
    .unwrap();
    assert_eq!(result.resets_at, Some(3_000.0));
    assert_eq!(result.limit_window.as_deref(), Some("five_hour"));
    assert!(result.blocks_new_work());
}

#[test]
fn selects_the_latest_reset_when_multiple_windows_are_exhausted() {
    let result = CodexQuotaRefreshResult::from_value(
        json!({
            "rateLimitsByLimitId": {
                "codex": {
                    "primary": {
                        "usedPercent": 100,
                        "windowDurationMins": 300,
                        "resetsAt": 1_060
                    },
                    "secondary": {
                        "usedPercent": 100,
                        "windowDurationMins": 10_080,
                        "resetsAt": 2_000
                    }
                }
            }
        }),
        1_000.0,
    )
    .expect("codex snapshot");
    assert_eq!(result.resets_at, Some(2_000.0));
    assert_eq!(result.limit_window.as_deref(), Some("weekly"));
}

#[test]
fn refreshes_plain_limit_from_the_newest_local_rollout_tail() {
    let temp = tempfile::tempdir().unwrap();
    let rollout_dir = temp.path().join("sessions/2026/08/23");
    std::fs::create_dir_all(&rollout_dir).unwrap();
    let path = rollout_dir.join("rollout-thread-1.jsonl");
    let fixture = include_bytes!("../../../tests/fixtures/provider_events/codex-rate-limit.jsonl");
    std::fs::write(&path, fixture).unwrap();

    let result = refresh_from_rollout(temp.path(), Some("thread-1"), 1_000.0)
        .unwrap()
        .expect("local rollout quota");
    assert_eq!(result.resets_at, Some(2_000.0));
    assert_eq!(result.limit_window.as_deref(), Some("weekly"));
}

#[test]
fn refresh_ignores_a_rollout_that_does_not_match_the_running_session() {
    let temp = tempfile::tempdir().unwrap();
    let rollout_dir = temp.path().join("sessions/2026/08/23");
    std::fs::create_dir_all(&rollout_dir).unwrap();
    let path = rollout_dir.join("rollout-other.jsonl");
    let fixture = include_bytes!("../../../tests/fixtures/provider_events/codex-rate-limit.jsonl");
    std::fs::write(&path, fixture).unwrap();

    assert!(refresh_from_rollout(temp.path(), Some("running"), 1_000.0)
        .unwrap()
        .is_none());
}

#[test]
fn refresh_reads_only_the_bounded_tail_of_a_large_rollout() {
    let temp = tempfile::tempdir().unwrap();
    let rollout_dir = temp.path().join("sessions/2026/08/23");
    std::fs::create_dir_all(&rollout_dir).unwrap();
    let path = rollout_dir.join("rollout-large.jsonl");
    let fixture = include_bytes!("../../../tests/fixtures/provider_events/codex-rate-limit.jsonl");
    let mut bytes = vec![b'x'; CODEX_ROLLOUT_TAIL_BYTES + 1024];
    bytes.extend_from_slice(b"\n");
    bytes.extend_from_slice(fixture);
    std::fs::write(&path, bytes).unwrap();

    let result = refresh_from_rollout(temp.path(), Some("large"), 1_000.0)
        .unwrap()
        .expect("tail fixture quota");
    assert_eq!(result.resets_at, Some(2_000.0));
}
