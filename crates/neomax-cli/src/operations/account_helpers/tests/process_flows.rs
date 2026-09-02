use std::ffi::{OsStr, OsString};
use std::sync::{Arc, Mutex};

use neomax_core::Engine;

use super::super::profiles::DetectedAuth;
use super::super::run_with_ports;
use super::support::{FakeAuth, FakeProcess, context, profile};

#[test]
fn numeric_login_creates_first_use_profile_and_invokes_codex_login() {
    let (_temp, context) = context();
    let ensured = Arc::new(Mutex::new(Vec::new()));
    let auth = FakeAuth {
        profiles: vec![profile(Engine::Codex, "2", None)],
        ensured: Arc::clone(&ensured),
    };
    let process = FakeProcess::successful();
    run_with_ports(Engine::Codex, &["2".into()], &context, &auth, &process).unwrap();
    assert_eq!(&*ensured.lock().unwrap(), &["2".to_string()]);
    let calls = process.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].args, vec![OsString::from("login")]);
    assert!(calls[0].interactive);
    assert_eq!(
        calls[0].environment.get(OsStr::new("CODEX_HOME")),
        Some(&OsString::from("/fixture/codex/2"))
    );
}

#[test]
fn kimi_api_key_run_omits_implicit_model_but_forwards_explicit_model() {
    let (_temp, context) = context();
    let auth = FakeAuth {
        profiles: vec![profile(Engine::Kimi, "1", Some(DetectedAuth::ApiKey))],
        ensured: Arc::new(Mutex::new(Vec::new())),
    };
    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Kimi,
        &["run".into(), "--prompt".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    let calls = process.calls.lock().unwrap();
    assert!(!calls[0].args.contains(&OsString::from("-m")));
    drop(calls);

    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Kimi,
        &[
            "run".into(),
            "--model".into(),
            "kimi-code/k2.7".into(),
            "--prompt".into(),
        ],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    let calls = process.calls.lock().unwrap();
    assert!(
        calls[0]
            .args
            .windows(2)
            .any(|pair| { pair == [OsString::from("-m"), OsString::from("kimi-code/k2.7")] })
    );
}

#[test]
fn grok_device_login_and_opencode_models_use_fake_process_only() {
    let (_temp, context) = context();
    let ensured = Arc::new(Mutex::new(Vec::new()));
    let auth = FakeAuth {
        profiles: vec![
            profile(Engine::Grok, "orch", None),
            profile(Engine::Opencode, "1", Some(DetectedAuth::ApiKey)),
        ],
        ensured: Arc::clone(&ensured),
    };
    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Grok,
        &["login".into(), "orch".into(), "device".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    assert_eq!(
        process.calls.lock().unwrap()[0].args,
        vec![OsString::from("login"), OsString::from("--device-auth")]
    );

    let process = FakeProcess::successful();
    run_with_ports(
        Engine::Opencode,
        &["models".into(), "--json".into()],
        &context,
        &auth,
        &process,
    )
    .unwrap();
    assert_eq!(
        process.calls.lock().unwrap()[0].args.first(),
        Some(&OsString::from("models"))
    );
    assert_eq!(
        &*ensured.lock().unwrap(),
        &["orch".to_string(), "1".to_string()]
    );
}

#[test]
fn model_discovery_ensures_new_profiles_for_each_supported_provider() {
    for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
        let (_temp, context) = context();
        let ensured = Arc::new(Mutex::new(Vec::new()));
        let auth = FakeAuth {
            profiles: vec![profile(engine, "2", None)],
            ensured: Arc::clone(&ensured),
        };
        let process = FakeProcess::successful();
        run_with_ports(
            engine,
            &["models".into(), "2".into()],
            &context,
            &auth,
            &process,
        )
        .unwrap();
        assert_eq!(&*ensured.lock().unwrap(), &["2".to_string()]);
    }
}
