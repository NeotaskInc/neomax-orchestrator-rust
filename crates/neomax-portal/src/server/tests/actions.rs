use super::super::PortalServer;
use super::fixtures::{EmptySource, RecordingExecutor, StubPrState, request};
use crate::address::LocalBind;

#[test]
fn local_actions_require_same_origin_and_are_injected_for_hermetic_tests() {
    let executor = RecordingExecutor::default();
    let server =
        PortalServer::new(LocalBind::loopback(8787), EmptySource).with_action_executor(executor);
    let denied = server
        .response(&request(
            "POST",
            "/api/pause/kimi/2",
            b"{}",
            Some("http://evil.test"),
        ))
        .unwrap();
    assert_eq!(denied.status, 403);

    let response = server
        .response(&request(
            "POST",
            "/api/pause/kimi/2",
            b"{}",
            Some("http://127.0.0.1:8787"),
        ))
        .unwrap();
    assert_eq!(response.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["operation"], "pause");
    assert_eq!(body["executed"], true);
}

#[test]
fn general_connect_action_is_planned_without_invoking_a_provider() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource)
        .with_action_executor(RecordingExecutor::default());
    let response = server
        .response(&request(
            "POST",
            "/api/action",
            br#"{"action":"connect","engine":"opencode","account":"2"}"#,
            Some("http://127.0.0.1:8787"),
        ))
        .unwrap();
    assert_eq!(response.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["operation"], "connect");
    assert_eq!(body["plan"]["program"], "ocx");
    assert_eq!(body["plan"]["args"], serde_json::json!(["login", "2"]));
}

#[test]
fn destructive_action_returns_conflict_until_confirmed() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource)
        .with_action_executor(RecordingExecutor::default());
    let response = server
        .response(&request(
            "POST",
            "/api/act/kill/run-1",
            b"{}",
            Some("http://127.0.0.1:8787"),
        ))
        .unwrap();
    assert_eq!(response.status, 409);
    let response = server
        .response(&request(
            "POST",
            "/api/act/kill/run-1",
            br#"{"confirm":true}"#,
            Some("http://127.0.0.1:8787"),
        ))
        .unwrap();
    assert_eq!(response.status, 200);
}

#[test]
fn prstate_uses_injectable_resolver_and_validates_url() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource)
        .with_pr_state_resolver(StubPrState);
    let response = server
        .response(&request(
            "GET",
            "/api/prstate?url=https://github.com/NeotaskInc/neomax/pull/42",
            &[],
            None,
        ))
        .unwrap();
    assert_eq!(response.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["state"], "OPEN");
}
