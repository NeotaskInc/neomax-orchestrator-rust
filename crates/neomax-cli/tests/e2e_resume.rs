#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;
use std::path::Path;

use neomax_core::Engine;

use support::E2eHarness;

fn seed_claude(profile: &Path, id: &str) {
    let directory = profile.join("projects/fixture");
    fs::create_dir_all(&directory).expect("Claude session directory");
    fs::write(
        directory.join(format!("{id}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"sessionId\":\"{id}\",\"cwd\":\"/workspace\",\"message\":{{\"content\":\"fixture\"}}}}\n"
        ),
    )
    .expect("Claude session artifact");
}

fn seed_codex(profile: &Path, id: &str) {
    let directory = profile.join("sessions/2026/08");
    fs::create_dir_all(&directory).expect("Codex session directory");
    fs::write(
        directory.join(format!("rollout-{id}.jsonl")),
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/workspace\"}}}}\n"
        ),
    )
    .expect("Codex session artifact");
}

fn seed_kimi(profile: &Path, id: &str) {
    let directory = profile.join(format!("sessions/{id}"));
    fs::create_dir_all(&directory).expect("Kimi session directory");
    fs::write(
        directory.join("state.json"),
        serde_json::json!({
            "sessionId": id,
            "workDir": "/workspace",
            "agents": {"main": {"type": "main"}}
        })
        .to_string(),
    )
    .expect("Kimi session artifact");
}

fn seed_grok(profile: &Path, id: &str) {
    let directory = profile.join(format!("sessions/{id}"));
    fs::create_dir_all(&directory).expect("Grok session directory");
    fs::write(
        directory.join("summary.json"),
        serde_json::json!({"info": {"id": id, "cwd": "/workspace"}}).to_string(),
    )
    .expect("Grok session artifact");
}

#[test]
fn exact_and_prefix_resume_select_the_owning_provider_account() {
    let harness = E2eHarness::new([Engine::Claude, Engine::Codex]);
    seed_claude(harness.profile(Engine::Claude, 0), "claude-session");
    seed_codex(harness.profile(Engine::Codex, 0), "codex-session");

    let exact = harness.run([
        "--json",
        "--foreground",
        "--resume",
        "--session-id",
        "claude-session",
    ]);
    let report = exact.json();
    assert_eq!(report["engine"], "claude");
    assert_eq!(report["session"], "claude-session");
    let invocation = harness.invocations().pop().expect("Claude invocation");
    assert!(invocation.has_arg("--resume"));
    assert!(invocation.has_arg("claude-session"));

    let prefix = harness.run_alias(
        "cdxmax",
        ["--json", "--foreground", "launch", "resume", "codex-sess"],
    );
    let report = prefix.json();
    assert_eq!(report["engine"], "codex");
    assert_eq!(report["session"], "codex-session");
    let invocation = harness.invocations().pop().expect("Codex invocation");
    assert!(invocation.has_arg("resume"));
    assert!(invocation.has_arg("codex-session"));
    harness.assert_hermetic_invocations();
}

#[test]
fn ambiguous_resume_prefix_fails_before_any_provider_process() {
    let harness = E2eHarness::new([Engine::Claude, Engine::Codex]);
    seed_claude(harness.profile(Engine::Claude, 0), "shared-claude");
    seed_codex(harness.profile(Engine::Codex, 0), "shared-codex");

    let result = harness.run(["--json", "--foreground", "launch", "resume", "shared"]);
    assert!(!result.status.success());
    assert!(result.stderr.contains("ambiguous"));
    assert!(harness.invocations().is_empty());
}

#[test]
fn kimi_and_grok_resume_use_their_native_session_switches() {
    let harness = E2eHarness::new([Engine::Kimi, Engine::Grok]);
    seed_kimi(harness.profile(Engine::Kimi, 0), "kimi-session");
    seed_grok(harness.profile(Engine::Grok, 0), "grok-session");

    let kimi = harness.run_alias(
        "kmax",
        ["--json", "--foreground", "launch", "resume", "kimi-session"],
    );
    kimi.json();
    let grok = harness.run_alias(
        "gmax",
        ["--json", "--foreground", "launch", "resume", "grok-session"],
    );
    grok.json();

    let invocations = harness.invocations();
    let kimi = invocations
        .iter()
        .find(|invocation| invocation.field("provider") == Some("kimi"))
        .expect("Kimi invocation");
    assert!(kimi.has_arg("-S"));
    assert!(kimi.has_arg("kimi-session"));
    let grok = invocations
        .iter()
        .find(|invocation| invocation.field("provider") == Some("grok"))
        .expect("Grok invocation");
    assert!(grok.has_arg("--resume"));
    assert!(grok.has_arg("grok-session"));
    harness.assert_hermetic_invocations();
}

#[test]
fn provider_alias_resume_forms_are_native_and_engine_scoped() {
    let harness = E2eHarness::new([Engine::Claude, Engine::Codex, Engine::Kimi, Engine::Grok]);
    seed_claude(harness.profile(Engine::Claude, 0), "claude-direct");
    seed_codex(harness.profile(Engine::Codex, 0), "codex-direct");
    seed_kimi(harness.profile(Engine::Kimi, 0), "kimi-direct");
    seed_grok(harness.profile(Engine::Grok, 0), "grok-direct");

    for (alias, id, native_switch) in [
        ("cmax", "claude-direct", "--resume"),
        ("cdxmax", "codex-direct", "resume"),
        ("kmax", "kimi-direct", "-S"),
        ("gmax", "grok-direct", "--resume"),
    ] {
        let result = harness.run_alias(alias, ["--json", "--foreground", "resume", id]);
        assert_eq!(result.json()["session"], id, "{alias} positional resume");
        let invocation = harness
            .invocations()
            .into_iter()
            .find(|invocation| invocation.field("provider") == Some(native_provider(alias)))
            .unwrap_or_else(|| panic!("{alias} did not invoke its provider"));
        assert!(
            invocation.has_arg(native_switch),
            "{alias} native resume switch"
        );
    }

    for (alias, id) in [
        ("cmax", "claude-direct"),
        ("cdxmax", "codex-direct"),
        ("kmax", "kimi-direct"),
        ("gmax", "grok-direct"),
    ] {
        let result = harness.run_alias(alias, ["--json", "--foreground", "--resume", id]);
        assert_eq!(result.json()["session"], id, "{alias} flag resume");
    }
    harness.assert_hermetic_invocations();
}

fn native_provider(alias: &str) -> &'static str {
    match alias {
        "cmax" => "claude",
        "cdxmax" => "codex",
        "kmax" => "kimi",
        "gmax" => "grok",
        _ => unreachable!("unknown provider alias"),
    }
}
