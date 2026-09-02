use std::fs;

use neomax_core::config::Engine;

use super::*;
use crate::state::WatchState;
use crate::test_support::agent_paths;

#[test]
fn incremental_sweep_preserves_partial_lines_and_captures_all_text_providers() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let claude = paths.home.join(".claude").join("projects").join("demo");
    let codex = paths.home.join(".codex").join("sessions").join("2026");
    let kimi = paths
        .home
        .join(".kimi-code")
        .join("sessions")
        .join("session")
        .join("agents")
        .join("main");
    fs::create_dir_all(&claude).unwrap();
    fs::create_dir_all(&codex).unwrap();
    fs::create_dir_all(&kimi).unwrap();
    fs::write(
        claude.join("one.jsonl"),
        r#"{"timestamp":"2026-05-30T12:00:00Z","sessionId":"s","message":{"role":"assistant","id":"m1","model":"claude-fable-5","usage":{"input_tokens":10,"output_tokens":3}}}
"#,
    )
    .unwrap();
    fs::write(
        codex.join("one.jsonl"),
        r#"{"timestamp":"2026-05-30T12:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":2,"output_tokens":4}}}}
"#,
    )
    .unwrap();
    fs::write(
        kimi.join("wire.jsonl"),
        r#"{"type":"usage.record","time":1800000000000,"model":"kimi-code/k3","usage":{"inputOther":4,"output":5}}
"#,
    )
    .unwrap();
    let collector = UsageCollector::with_now(paths.clone(), 1_800_000_000);
    let mut state = WatchState::default();
    let report = collector.sweep(&mut state, SweepMode::Full, 0).unwrap();
    assert_eq!(report.records_emitted, 3);
    assert_eq!(report.providers.len(), 5);
    assert!(
        report
            .providers
            .iter()
            .any(|item| item.provider == Engine::Claude)
    );
    assert!(
        report
            .providers
            .iter()
            .any(|item| item.provider == Engine::Codex)
    );
    assert!(
        report
            .providers
            .iter()
            .any(|item| item.provider == Engine::Kimi)
    );
}

#[test]
fn codex_cumulative_records_only_emit_the_new_high_water_mark() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".codex").join("sessions").join("2026");
    fs::create_dir_all(&root).unwrap();
    let line = |output: u64| {
        format!(
            r#"{{"payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":20,"cached_input_tokens":2,"output_tokens":{output}}}}}}}}}
"#
        )
    };
    fs::write(root.join("run.jsonl"), format!("{}{}", line(2), line(5))).unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut state = WatchState::default();
    let report = collector.sweep(&mut state, SweepMode::Full, 0).unwrap();
    assert_eq!(report.records_emitted, 2);
    assert_eq!(state.codex_total.len(), 1);
    let report = collector
        .sweep(&mut state, SweepMode::Incremental, 0)
        .unwrap();
    assert_eq!(report.records_emitted, 0);
}

#[test]
fn oversized_partial_source_makes_bounded_progress() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".claude").join("projects").join("demo");
    fs::create_dir_all(&root).unwrap();
    let transcript = root.join("oversized.jsonl");
    fs::write(
        &transcript,
        vec![b'{'; crate::io::MAX_SOURCE_BYTES_PER_SWEEP + 128],
    )
    .unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut state = WatchState::default();

    collector
        .sweep(&mut state, SweepMode::Full, 0)
        .expect("bounded source scan");
    assert!(
        state
            .files
            .get(&source_key(&transcript))
            .is_some_and(|offset| *offset > 0)
    );
}

#[test]
fn rate_limit_events_trigger_post_collection_refresh_signal() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".codex").join("sessions").join("2026");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("limited.jsonl"),
        r#"{"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":4}},"rate_limits":{"primary":{"used_percent":99}}}}
"#,
    )
    .unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut state = WatchState::default();
    let report = collector
        .sweep(&mut state, SweepMode::Full, 0)
        .expect("rate-limit source scan");
    assert_eq!(report.rate_limits, 1);
}

#[test]
fn rate_limit_totals_saturate_at_u64_max() {
    assert_eq!(saturating_sum([u64::MAX, 1].into_iter()), u64::MAX);

    let mut report = SweepReport::default();
    report.add_provider(ProviderSweep {
        rate_limits: u64::MAX,
        ..ProviderSweep::default()
    });
    report.add_provider(ProviderSweep {
        rate_limits: 1,
        ..ProviderSweep::default()
    });

    assert_eq!(report.rate_limits, u64::MAX);
    assert_eq!(report.providers[0].rate_limits, u64::MAX);
}
