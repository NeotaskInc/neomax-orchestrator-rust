use serde_json::json;

use super::super::super::codex_quota::refresh_request;
use super::super::super::common::stream;
use super::super::parse_at;

#[test]
fn records_rollout_rate_limit_reset_metadata() {
    let output = parse_at(
        &stream(&[json!({
            "type":"event_msg",
            "payload":{
                "type":"token_count",
                "info":{"total_token_usage":{"input_tokens":10,"output_tokens":2}},
                "rate_limits":{"primary":{"used_percent":100,"window_minutes":10080,"resets_at":2_000},"secondary":null}
            }
        })]),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.api_error_status.as_deref(), Some("429"));
    assert_eq!(output.resets_at, Some(2_000.0));
    assert_eq!(output.limit_window.as_deref(), Some("weekly"));
    assert!(refresh_request(&output).is_some());
}

#[test]
fn parses_checked_in_rate_limit_fixture() {
    let output = parse_at(
        include_bytes!("../../../../../tests/fixtures/provider_events/codex-rate-limit.jsonl"),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.api_error_status.as_deref(), Some("429"));
    assert_eq!(output.resets_at, Some(2_000.0));
    assert_eq!(output.limit_window.as_deref(), Some("weekly"));
}

#[test]
fn treats_ninety_nine_percent_snapshot_as_rotation_boundary() {
    let output = parse_at(
        &stream(&[json!({
            "type":"account/rateLimits/updated",
            "params":{"rateLimits":{"primary":{"usedPercent":99,"windowDurationMins":300,"resetsAt":2_000}}}
        })]),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.api_error_status.as_deref(), Some("429"));
    assert_eq!(output.resets_at, Some(2_000.0));
}

#[test]
fn accepts_raw_app_server_rate_limit_notifications() {
    let output = parse_at(
        &stream(&[json!({
            "method": "account/rateLimits/updated",
            "params": {
                "rateLimits": {
                    "primary": {
                        "usedPercent": 100,
                        "windowDurationMins": 300,
                        "resetsAt": 2_000
                    }
                }
            }
        })]),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.resets_at, Some(2_000.0));
}

#[test]
fn reads_nested_error_rate_limit_metadata() {
    let output = parse_at(
        &stream(&[json!({
            "type":"turn.failed",
            "error":{
                "data":{
                    "statusCode":429,
                    "rateLimits":{"primary":{"usedPercent":100,"windowDurationMins":300,"resetsAt":2_000}}
                }
            }
        })]),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.api_error_status.as_deref(), Some("429"));
    assert_eq!(output.resets_at, Some(2_000.0));
    assert_eq!(output.limit_window.as_deref(), Some("five_hour"));
}

#[test]
fn records_retry_metadata_from_a_rate_limit_error_without_a_quota_snapshot() {
    let output = parse_at(
        &stream(&[json!({
            "type": "error",
            "status": 429,
            "headers": {"retry-after": "90"},
            "message": "usage limit reached"
        })]),
        1_000.0,
    );
    assert!(output.rate_limited);
    assert_eq!(output.api_error_status.as_deref(), Some("429"));
    assert_eq!(output.resets_at, Some(1_090.0));
}
