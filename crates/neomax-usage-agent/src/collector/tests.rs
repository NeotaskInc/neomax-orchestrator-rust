use std::fs;

use neomax_core::config::Engine;

use super::*;
use crate::state::WatchState;
use crate::test_support::agent_paths;

#[test]
fn live_collector_reads_time_at_each_sweep_and_fixed_clock_remains_injectable() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let collector = UsageCollector::new(paths.clone());
    assert!(collector.now.is_none());
    let mut state = WatchState::default();
    let before = Utc::now().timestamp();
    collector
        .sweep(&mut state, SweepMode::Incremental, 2)
        .unwrap();
    let timestamp = state.extra["usage_import"]["updated_at"].as_i64().unwrap();
    assert!((before..=Utc::now().timestamp()).contains(&timestamp));
    UsageCollector::with_now(paths, 1_800_000_000)
        .sweep(&mut state, SweepMode::Incremental, 2)
        .unwrap();
    assert_eq!(state.extra["usage_import"]["updated_at"], 1_800_000_000);
}

#[test]
fn upgrades_legacy_claude_cache_metadata_without_duplicating_usage_or_rewinding_codex() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".claude/projects/demo");
    fs::create_dir_all(&root).unwrap();
    let file = root.join("legacy.jsonl");
    let line = r#"{"timestamp":"2026-09-01T12:00:00Z","message":{"role":"assistant","id":"native-message","model":"claude-fable-5","usage":{"input_tokens":10,"output_tokens":9,"cache_creation_input_tokens":1000000,"cache_creation":{"ephemeral_1h_input_tokens":1000000}}}}"#;
    fs::write(&file, format!("{line}\n")).unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut legacy = neomax_core::usage::parse_claude_line(line, "1", 1_800_000_000).unwrap();
    legacy.extra.clear();
    collector.ledger().append(&[legacy]).unwrap();
    let mut state = WatchState::default();
    state.extra.insert("usage_parser_version".into(), 2.into());
    state
        .files
        .insert(source_key(&file), fs::metadata(&file).unwrap().len());
    state
        .codex_model
        .insert("existing".into(), "gpt-6-astra".into());
    let first = collector
        .sweep(&mut state, SweepMode::Incremental, 2)
        .unwrap();
    assert_eq!(first.records_emitted, 1);
    assert_eq!(first.pending_files, 0);
    assert_eq!(state.codex_model["existing"], "gpt-6-astra");
    for rows in [
        collector.ledger().read_windowed(0, 1_800_000_000).unwrap(),
        collector
            .ledger()
            .read_deduplicated(0, 1_800_000_000)
            .unwrap(),
    ] {
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].output, 9);
        assert_eq!(rows[0].extra["cache_write_1h"], 1_000_000);
        let report = neomax_core::usage::build_usage_report(
            &rows,
            0,
            1_800_000_000,
            &neomax_core::usage::PriceCatalog::default(),
        );
        assert_eq!(report.grand.cost, 20.0);
    }
    assert_eq!(
        collector
            .sweep(&mut state, SweepMode::Incremental, 2)
            .unwrap()
            .records_emitted,
        0
    );
}

#[test]
fn older_standalone_history_finishes_importing_after_the_first_chunk() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".claude-solo/projects/demo");
    fs::create_dir_all(&root).unwrap();
    let file = root.join("direct-cli.jsonl");
    let mut bytes = b"{}\n".repeat(crate::io::MAX_SOURCE_BYTES_PER_SWEEP / 3 + 1);
    bytes.extend_from_slice(b"{\"timestamp\":\"2026-09-01T12:00:00Z\",\"message\":{\"role\":\"assistant\",\"id\":\"native-message\",\"model\":\"claude-fable-5\",\"usage\":{\"input_tokens\":10,\"output_tokens\":9}}}\n");
    fs::write(&file, bytes).unwrap();
    fs::File::open(&file)
        .unwrap()
        .set_times(
            fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            ),
        )
        .unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut state = WatchState::default();
    let first = collector.sweep(&mut state, SweepMode::Full, 2).unwrap();
    assert_eq!(first.pending_files, 1);
    assert!(first.pending_bytes > 0);
    let second = collector
        .sweep(&mut state, SweepMode::Incremental, 2)
        .unwrap();
    assert_eq!(second.records_emitted, 1);
    assert_eq!(second.pending_files, 0);
    assert_eq!(
        state.files[&source_key(&file)],
        fs::metadata(&file).unwrap().len()
    );
    assert_eq!(
        collector.ledger().read_windowed(0, 1_800_000_000).unwrap()[0].output,
        9
    );
}

#[test]
fn archived_direct_codex_usage_preserves_non_gpt_models_without_authentication() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".codex-personal/archived_sessions");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("native.jsonl"), concat!(
        "{\"type\":\"turn_context\",\"payload\":{\"model\":\"router/custom-model\"}}\n",
        "{\"type\":\"response_item\",\"payload\":{\"model\":\"gpt-unrelated-tool-output\"}}\n",
        "{\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":20,\"output_tokens\":4}}}}\n"
    )).unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let mut state = WatchState::default();
    state.files.insert(
        source_key(&root.join("native.jsonl")),
        fs::metadata(root.join("native.jsonl")).unwrap().len(),
    );
    collector
        .sweep(&mut state, SweepMode::Incremental, 2)
        .unwrap();
    let rows = collector.ledger().read_windowed(0, 1_800_000_000).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].model, "router/custom-model");
    assert_eq!(rows[0].extra["usage_parser_version"], 2);
    assert!(!root.parent().unwrap().join("auth.json").exists());
    assert_eq!(
        collector
            .sweep(&mut state, SweepMode::Incremental, 2)
            .unwrap()
            .records_emitted,
        0
    );
}

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
fn codex_counter_restart_keeps_the_new_request_without_a_quota_event() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let root = paths.home.join(".codex").join("sessions");
    fs::create_dir_all(&root).unwrap();
    let rows = [1000, 20]
        .into_iter()
        .map(|input| {
            serde_json::json!({
                "payload":{"type":"token_count","info":{
                    "total_token_usage":{"input_tokens":input,"output_tokens":4},
                    "last_token_usage":{"input_tokens":20,"output_tokens":4}
                }}
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(root.join("restart.jsonl"), rows).unwrap();
    let collector = UsageCollector::with_now(paths, 1_800_000_000);
    let report = collector
        .sweep(&mut WatchState::default(), SweepMode::Full, 0)
        .unwrap();
    assert_eq!(report.records_emitted, 2);
    assert_eq!(report.rate_limits, 0);
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
