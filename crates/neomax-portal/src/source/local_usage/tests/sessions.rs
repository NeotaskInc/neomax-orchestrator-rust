use std::fs;

use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::sessions::{SessionKind, SessionRecord};

use super::super::sessions::detail;

#[test]
fn kimi_fixture_publishes_wire_models_and_native_agents() {
    let temp = tempfile::tempdir().unwrap();
    let profile_path = temp.path().join(".kimi-code-acct1");
    fs::create_dir_all(&profile_path).unwrap();
    let profile = ProviderProfile {
        engine: Engine::Kimi,
        account: "1".into(),
        path: profile_path,
        reserved: false,
    };
    let mut main = SessionRecord::with_identity("s1", Engine::Kimi, "1");
    main.model = Some("kimi-code/k3".into());
    main.last_active = Some(100);
    main.tokens.output = 7;
    main.requests = 1;
    main.completions = 1;
    main.errors = 1;
    main.rate_limits = 1;
    main.tool_calls = 2;
    let mut child = SessionRecord::with_identity("agent-1", Engine::Kimi, "1");
    child.kind = SessionKind::NativeSubagent;
    child.parent_id = Some("s1".into());
    child.label = Some("review".into());
    child.model = Some("kimi-code/k2.7".into());
    child.last_active = Some(99);
    child.tokens.output = 4;
    child.requests = 1;
    child.completions = 1;
    child.tool_calls = 3;
    let result = detail(Engine::Kimi, &profile, 7, &[main, child], 0);
    assert_eq!(result.source, "kimi-local-wire");
    assert_eq!(result.totals.metrics.output, 7);
    assert_eq!(result.totals.sessions, 1);
    assert_eq!(
        result
            .models
            .iter()
            .map(|row| row.metrics.output)
            .sum::<u64>(),
        result.totals.metrics.output
    );
    assert_eq!(result.models.len(), 2);
    assert_eq!(result.totals.tool_calls, 2);
    assert_eq!(result.tool_usage.len(), 1);
    assert_eq!(
        result.last_error.as_ref().unwrap().status.as_deref(),
        Some("429")
    );
    assert_eq!(result.agents[0].agent, "review");
}

#[test]
fn grok_fixture_publishes_persisted_models_and_native_agents() {
    let temp = tempfile::tempdir().unwrap();
    let profile_path = temp.path().join(".grok-acct1");
    fs::create_dir_all(&profile_path).unwrap();
    let profile = ProviderProfile {
        engine: Engine::Grok,
        account: "1".into(),
        path: profile_path,
        reserved: false,
    };
    let mut main = SessionRecord::with_identity("s1", Engine::Grok, "1");
    main.model = Some("grok-4.6".into());
    main.last_active = Some(100);
    main.tokens.output = 8;
    main.requests = 1;
    main.completions = 1;
    let mut child = SessionRecord::with_identity("agent-1", Engine::Grok, "1");
    child.kind = SessionKind::NativeSubagent;
    child.parent_id = Some("s1".into());
    child.label = Some("inspect".into());
    child.last_active = Some(99);
    let result = detail(Engine::Grok, &profile, 7, &[main, child], 0);
    assert_eq!(result.source, "grok-local-jsonl");
    assert_eq!(result.totals.metrics.output, 8);
    assert_eq!(result.totals.native_subagents, 1);
    assert_eq!(result.agents[0].agent, "inspect");
}

#[test]
fn session_detail_keeps_provider_native_children_in_agent_rows() {
    let temp = tempfile::tempdir().unwrap();
    let profile = ProviderProfile {
        engine: Engine::Grok,
        account: "1".into(),
        path: temp.path().join(".grok"),
        reserved: false,
    };
    fs::create_dir_all(&profile.path).unwrap();
    let mut main = SessionRecord::with_identity("main", Engine::Grok, "1");
    main.model = Some("grok-4.6".into());
    main.requests = 1;
    main.completions = 1;
    main.tool_calls = 2;
    let mut child = SessionRecord::with_identity("child", Engine::Grok, "1");
    child.kind = SessionKind::NativeSubagent;
    child.parent_id = Some("main".into());
    child.model = Some("grok-4.6".into());
    child.tool_calls = 3;
    child.tool_errors = 1;
    child.errors = 1;
    child.rate_limits = 1;
    let result = detail(Engine::Grok, &profile, 7, &[main, child], 0);
    assert_eq!(result.totals.native_subagents, 1);
    assert_eq!(result.totals.tool_calls, 5);
    assert_eq!(result.totals.tool_errors, 1);
    assert_eq!(
        result.last_error.as_ref().unwrap().status.as_deref(),
        Some("429")
    );
    assert_eq!(result.agents.len(), 1);
}
