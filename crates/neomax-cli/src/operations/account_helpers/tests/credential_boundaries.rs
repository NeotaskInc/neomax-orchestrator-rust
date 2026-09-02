use std::sync::Arc;

use neomax_core::Engine;

use super::super::run_with_ports;
use super::support::{FakeAuth, FakeProcess, context, profile};

#[test]
fn grok_api_key_login_uses_auth_port_without_putting_a_secret_in_process_args() {
    let (_temp, context) = context();
    let auth = FakeAuth {
        profiles: vec![profile(Engine::Grok, "1", None)],
        ensured: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Grok,
        &["login".into(), "1".into(), "api-key".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    assert!(process.calls.lock().unwrap().is_empty());
}

#[test]
fn grok_choose_routes_to_the_selected_api_key_without_invoking_oauth() {
    let (_temp, context) = context();
    let auth = FakeAuth {
        profiles: vec![profile(Engine::Grok, "1", None)],
        ensured: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Grok,
        &["login".into(), "1".into(), "choose".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    assert!(process.calls.lock().unwrap().is_empty());
}

#[test]
fn kimi_api_key_login_uses_secret_boundary_without_invoking_cli() {
    let (_temp, context) = context();
    let auth = FakeAuth {
        profiles: vec![profile(Engine::Kimi, "1", None)],
        ensured: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Kimi,
        &["login".into(), "1".into(), "api-key".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    assert!(process.calls.lock().unwrap().is_empty());
}
