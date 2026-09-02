#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;
use neomax_core::orchestration::rotation::ArmedRotateStore;
use neomax_core::usage::{ProviderUsageCache, QuotaWindow, UsageCacheStore};
use std::fs;

use support::{
    E2eHarness,
    wait::{
        process_test_guard, terminate_pid, wait_for_exit, wait_for_pid_exit, wait_for_run,
        wait_for_run_or_child_exit,
    },
};

#[test]
fn rotate_moves_live_work_to_the_next_same_provider_account() {
    let _process_guard = process_test_guard();
    #[cfg(windows)]
    let started = std::time::Instant::now();
    let mut harness = E2eHarness::with_behavior([Engine::Claude], "rotate");
    let first_profile = harness.profile(Engine::Claude, 0).to_path_buf();
    let second_profile = harness.add_profile(Engine::Claude, 2);
    let mut child = harness.spawn_with_env(
        [
            "--foreground",
            "--worker-dispatch",
            "--engine",
            "claude",
            "1",
            "rotation fixture",
        ],
        harness.authorized_orchestrator_environment(),
    );
    let (id, running) = wait_for_run_or_child_exit(&harness, &mut child, |run| {
        run["status"] == "running" && run["worker_pid"].as_u64().is_some()
    });
    assert!(running["worker_pid"].as_u64().is_some());
    let rotated = harness.run(["rotate", id.as_str(), "--json"]);
    let report = rotated.json();
    let item = &report["rotated"][0];
    assert_eq!(item["run_id"], id);
    assert_eq!(item["source_engine"], "claude");
    assert_eq!(item["target_engine"], "claude");
    assert_eq!(item["target_account"], "2");
    assert_eq!(item["crosses_provider"], false);

    let status = wait_for_exit(&mut child);
    assert!(status.success());
    let (_, record) = wait_for_run(&harness, |run| {
        run["id"].as_str() == Some(id.as_str()) && run["status"] == "done"
    });
    assert_eq!(record["status"], "done");
    let canonical_first_profile = fs::canonicalize(&first_profile).expect("canonical profile");
    assert_eq!(
        record["profile"],
        canonical_first_profile.to_string_lossy().as_ref()
    );
    let first_profile_string = canonical_first_profile.to_string_lossy().into_owned();
    let invocations = harness.invocations();
    assert_eq!(invocations.len(), 2);
    assert!(
        invocations
            .iter()
            .all(|invocation| invocation.field("profile") == Some(first_profile_string.as_str()))
    );
    let rotation_log = fs::read_to_string(harness.state_paths().auth_rotations)
        .expect("same-provider rotation log");
    assert!(rotation_log.contains("\"operation\":\"swap\""));
    let second_account = second_profile
        .file_name()
        .and_then(|name| name.to_str())
        .expect("second account name");
    assert!(rotation_log.contains(second_account));
    harness.assert_hermetic_invocations();
    #[cfg(windows)]
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[test]
fn handoff_dry_run_emits_a_provider_pinned_plan_without_starting_a_provider() {
    let mut harness = E2eHarness::new([Engine::Kimi]);
    harness.add_profile(Engine::Kimi, 2);
    let result = harness.run([
        "handoff",
        "--engine",
        "kimi",
        "--from",
        "1",
        "--target-account",
        "2",
        "--reason",
        "quota",
        "--base",
        ".",
        "--dry-run",
        "--json",
    ]);
    let plan = result.json();
    assert_eq!(plan["source_account"], "1");
    assert_eq!(plan["target_account"], "2");
    assert_eq!(plan["reason"], "quota");
    assert_eq!(plan["plan"]["engine"], "kimi");
    assert_eq!(plan["plan"]["launcher"], "kmax");
    assert!(plan["plan"]["headless"].is_boolean());
    assert!(harness.invocations().is_empty());
}

#[test]
fn rotation_does_not_cross_provider_when_the_pinned_scope_has_no_eligible_account() {
    let _process_guard = process_test_guard();
    let harness = E2eHarness::with_behavior([Engine::Claude, Engine::Codex], "rotate");
    let mut child = harness.spawn_with_env(
        [
            "--foreground",
            "--worker-dispatch",
            "--engine",
            "claude",
            "1",
            "fixture",
        ],
        harness.authorized_orchestrator_environment(),
    );
    let (id, _) = wait_for_run(&harness, |run| {
        run["status"] == "running" && run["worker_pid"].as_u64().is_some()
    });
    let rotate = harness.run(["rotate", id.as_str(), "--workers", "claude", "--json"]);
    let report = rotate.json();
    let item = &report["rotated"][0];
    assert!(item["target_engine"].is_null());
    assert!(item["target_account"].is_null());
    assert_eq!(item["crosses_provider"], false);
    assert!(item["status"].as_str().is_some_and(|status| {
        status.contains("cross-provider") || status.contains("eligible")
    }));
    assert_eq!(harness.invocations().len(), 1);
    harness.assert_hermetic_invocations();
    let killed = harness.run(["kill", id.as_str(), "--json"]);
    killed.json();
    let _ = wait_for_exit(&mut child);
}

#[test]
fn armed_rotation_ticks_every_provider_without_crossing_provider_scope() {
    let _process_guard = process_test_guard();
    let mut harness = E2eHarness::new(Engine::ALL);
    for engine in Engine::ALL {
        harness.add_profile(engine, 2);
    }
    for alias in ["cmax", "cdxmax", "ocmax", "kmax", "gmax"] {
        harness.run_alias(alias, ["--help"]).assert_success();
    }
    let usage = UsageCacheStore::new(harness.state.join("usage"));
    let armed = ArmedRotateStore::in_state_dir(&harness.state);
    let now = chrono::Utc::now().timestamp();
    for engine in Engine::ALL {
        let profile = harness.profile(engine, 0).to_path_buf();
        let (five_hour, weekly) = match engine {
            Engine::Claude => (99.0, 0.0),
            Engine::Codex => (0.0, 99.0),
            Engine::Opencode | Engine::Kimi | Engine::Grok => (0.0, 99.0),
        };
        usage
            .save(
                engine,
                &profile,
                &ProviderUsageCache {
                    five_hour: QuotaWindow {
                        used_percent: Some(five_hour),
                        resets_at: Some(4_000_000_000.0),
                    },
                    seven_day: QuotaWindow {
                        used_percent: Some(weekly),
                        resets_at: Some(4_000_000_000.0),
                    },
                    source: Some(format!("{engine}-usage-event")),
                    observed_at: Some(now as f64),
                    ..ProviderUsageCache::default()
                },
            )
            .expect("quota fixture");
        armed
            .arm(&profile, 99.0, 99.0, &[], true, now)
            .expect("armed rotation fixture");
    }

    let report = harness.run(["rotate-tick", "--json"]).json();
    let reports = report["armed"].as_array().expect("armed reports");
    assert_eq!(reports.len(), Engine::ALL.len());
    for engine in Engine::ALL {
        let item = reports
            .iter()
            .find(|item| item["engine"] == engine.to_string())
            .expect("missing armed provider report");
        let status = item["status"].as_str().unwrap();
        if matches!(engine, Engine::Claude | Engine::Codex) {
            assert!(status.starts_with("rotated to"), "{engine}: {status}");
        } else {
            assert_eq!(
                status, "same-provider handoff started",
                "{engine}: {status}"
            );
        }
    }
}

#[test]
fn untracked_rotate_hands_off_all_providers_without_crossing_scope() {
    let _process_guard = process_test_guard();
    let cases = [
        (Engine::Claude, "cmax", "CLAUDE_CONFIG_DIR"),
        (Engine::Codex, "cdxmax", "CODEX_HOME"),
        (Engine::Opencode, "ocmax", "XDG_DATA_HOME"),
        (Engine::Kimi, "kmax", "KIMI_CODE_HOME"),
        (Engine::Grok, "gmax", "GROK_HOME"),
    ];
    for (engine, alias, config_env) in cases {
        let mut harness = E2eHarness::new([engine]);
        let source = harness.profile(engine, 0).to_path_buf();
        let target = harness.add_profile(engine, 2);
        harness.run_alias(alias, ["--help"]).assert_success();
        let mut environment = harness.authorized_orchestrator_environment();
        for (name, value) in &mut environment {
            if name == "NEOMAX_ROLE" {
                *value = engine.to_string();
            }
        }
        environment.push((config_env.into(), source.to_string_lossy().into_owned()));
        let result = harness.run_alias_with_env(alias, ["rotate", "--json"], environment);
        let report = result.json();
        let launched_pid = report["launched_pid"].as_u64().map(|pid| {
            u32::try_from(pid).expect("fixture launched pid fits the platform process id")
        });
        assert_eq!(report["source_account"], "1", "{engine} source");
        assert_eq!(report["target_account"], "2", "{engine} target");
        assert_eq!(report["plan"]["engine"], engine.to_string(), "{engine}");
        assert_eq!(report["plan"]["launcher"], alias, "{engine}");
        assert_eq!(report["continuation"], serde_json::Value::Null);
        assert!(report["launched_pid"].as_u64().is_some());
        let target_string = target.to_string_lossy().into_owned();
        for _ in 0..400 {
            if harness
                .invocations()
                .iter()
                .any(|invocation| invocation.field("profile") == Some(target_string.as_str()))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let invocations = harness.invocations();
        assert_eq!(invocations.len(), 1, "{engine} invocation count");
        assert_eq!(invocations[0].field("provider"), Some(engine.as_str()));
        assert_eq!(
            invocations[0].field("profile"),
            Some(target_string.as_str())
        );
        if let Some(pid) = launched_pid {
            terminate_pid(pid);
            wait_for_pid_exit(pid);
        }
        harness.assert_hermetic_invocations();
    }
}
