use super::super::PortalServer;
use super::fixtures::{EmptySource, RecordingExecutor, filesystem_source, request};
use crate::address::LocalBind;
use neomax_core::Engine;
use neomax_core::orchestration::auth::{RotationEvent, RotationLog};

#[test]
fn rejects_mutating_methods_before_source_access() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource);
    let request = request("DELETE", "/api/status", &[], None);
    assert_eq!(server.response(&request).unwrap().status, 405);
}

#[test]
fn every_endpoint_rejects_missing_and_rebinding_hosts() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource);
    let mut missing = request("GET", "/api/status", &[], None);
    missing.headers.remove("host");
    assert_eq!(server.response(&missing).unwrap().status, 403);

    let mut rebound = request("GET", "/api/status", &[], None);
    rebound
        .headers
        .insert("host".into(), "localhost.attacker.test".into());
    assert_eq!(server.response(&rebound).unwrap().status, 403);

    let mut wrong_port = request("GET", "/api/status", &[], None);
    wrong_port
        .headers
        .insert("host".into(), "127.0.0.1:8788".into());
    assert_eq!(server.response(&wrong_port).unwrap().status, 403);
}

#[test]
fn serves_status_without_network_or_provider_access() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource);
    let response = server
        .response(&request("GET", "/api/status", &[], None))
        .unwrap();
    assert_eq!(response.status, 200);
    assert!(
        String::from_utf8(response.body)
            .unwrap()
            .contains("\"engines\"")
    );
}

#[test]
fn status_exposes_only_sanitized_recent_rotation_history() {
    let temp = tempfile::tempdir().unwrap();
    let source = filesystem_source(&temp);
    let log = RotationLog::new(source.paths().auth_rotations.clone());
    let now = chrono::Utc::now().timestamp();
    log.append(&RotationEvent {
        ts: now - 30,
        engine: Engine::Claude,
        operation: "swap".into(),
        destination: "/private/accounts/.claude-2".into(),
        source: Some("/private/accounts/.claude-1".into()),
        from_email: Some("from@example.test".into()),
        to_email: Some("to@example.test".into()),
        reason: Some("quota".into()),
    })
    .unwrap();
    let server = PortalServer::new(LocalBind::loopback(8787), source);
    let response = server
        .response(&request("GET", "/api/status", &[], None))
        .unwrap();
    assert_eq!(response.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    let rotations = body["summary"]["auth_rotations"].as_array().unwrap();
    assert_eq!(rotations.len(), 1);
    assert_eq!(rotations[0]["destination"], ".claude-2");
    assert_eq!(rotations[0]["source"], ".claude-1");
    assert_eq!(rotations[0]["reason"], "quota");
    let body_text = String::from_utf8(response.body).unwrap();
    assert!(!body_text.contains("from_email"));
    assert!(!body_text.contains("from@example.test"));
    assert!(!body_text.contains("/private/accounts"));
}

#[test]
fn action_posts_require_json_and_never_echo_internal_input_errors() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource)
        .with_action_executor(RecordingExecutor::default());
    let mut missing_type = request(
        "POST",
        "/api/pause/kimi/2",
        b"{}",
        Some("http://localhost:8787"),
    );
    missing_type.headers.remove("content-type");
    assert_eq!(server.response(&missing_type).unwrap().status, 415);

    let response = server
        .response(&request(
            "POST",
            "/api/action",
            br#"{"action":"secret-token-value"}"#,
            Some("http://localhost:8787"),
        ))
        .unwrap();
    assert_eq!(response.status, 400);
    let body = String::from_utf8(response.body).unwrap();
    assert!(!body.contains("secret-token-value"));
    assert_eq!(body, r#"{"error":"invalid action request"}"#);
}
