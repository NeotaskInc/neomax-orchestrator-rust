use std::fs;

use neomax_core::Engine;
use neomax_core::{ConcurrencySettings, EffectiveSettings, SettingsFile};
use serde_json::json;

use super::super::args::PlanAction;
use super::super::args::{RunAllInput, normalize_run_all, parse_action};

#[test]
fn run_all_arguments_normalize_a_plan_without_provider_calls() {
    let fixture = tempfile::tempdir().unwrap();
    let plan_path = fixture.path().join("batch.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&json!({
            "plan": "batch-1",
            "repo": "repo",
            "parts": [{"id": "one", "prompt": "inspect", "engine": "opencode"}]
        }))
        .unwrap(),
    )
    .unwrap();
    let parsed = parse_action(
        PlanAction::RunAll,
        &[
            plan_path.display().to_string(),
            "--workers".into(),
            "opencode".into(),
            "--max-live".into(),
            "2".into(),
        ],
    )
    .unwrap();
    let arguments = normalize_run_all(RunAllInput {
        path: parsed.plan_path.unwrap(),
        cwd: fixture.path().to_path_buf(),
        scope: parsed.scope,
        runtime: parsed.runtime,
        repository: parsed.repository,
        base: parsed.base,
        integration_branch: parsed.integration_branch,
        plan_id: parsed.plan_id,
    })
    .unwrap();
    assert_eq!(arguments.plan_id, "batch-1");
    assert_eq!(arguments.loaded.plan.parts[0].engine, Engine::Opencode);
    assert_eq!(arguments.runtime.runtime.max_live, 2);
    assert!(arguments.runtime.max_live_explicit);
}

#[test]
fn repeated_runs_of_the_same_plan_file_get_distinct_durable_ids() {
    let fixture = tempfile::tempdir().unwrap();
    let plan_path = fixture.path().join("batch.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&json!({
            "parts": [{"id": "one", "prompt": "inspect", "engine": "opencode"}]
        }))
        .unwrap(),
    )
    .unwrap();
    let input = || RunAllInput {
        path: plan_path.clone(),
        cwd: fixture.path().to_path_buf(),
        scope: "opencode".parse().unwrap(),
        runtime: Default::default(),
        repository: None,
        base: None,
        integration_branch: None,
        plan_id: None,
    };

    let first = normalize_run_all(input()).unwrap();
    let second = normalize_run_all(input()).unwrap();
    assert_ne!(first.plan_id, second.plan_id);
    assert!(first.plan_id.starts_with("plan-batch-"));
    assert!(second.plan_id.starts_with("plan-batch-"));
}

#[test]
fn generated_identity_is_stable_for_fixed_invocation_inputs_and_sequence_safe() {
    let path = std::path::Path::new("batch.json");
    let first = super::super::args::default_plan_id_with_identity(path, 42, 7, 0);
    let same = super::super::args::default_plan_id_with_identity(path, 42, 7, 0);
    let next = super::super::args::default_plan_id_with_identity(path, 42, 7, 1);
    assert_eq!(first, same);
    assert_ne!(first, next);
}

#[test]
fn generated_identity_stays_within_persistent_plan_id_limits() {
    let path = std::path::PathBuf::from(format!("{}.json", "a".repeat(200)));
    let id = super::super::args::default_plan_id_with_identity(&path, 42, 7, 0);
    assert!(id.len() <= 128);
    neomax_core::scheduler::persistence::validate_plan_id(&id).unwrap();
}

#[test]
fn run_all_default_capacity_uses_effective_settings_without_provider_calls() {
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 20,
                lanes_per_account: 4,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        "config.toml".into(),
        &Default::default(),
    )
    .unwrap();
    let options = super::super::types::PlanRuntimeOptions::default()
        .resolve_run_all(&settings, 3)
        .unwrap();
    assert_eq!(options.runtime.max_live, 12);
    assert!(!options.max_live_explicit);
}

#[test]
fn explicit_max_live_over_the_subagent_budget_is_rejected() {
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 5,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        "config.toml".into(),
        &Default::default(),
    )
    .unwrap();
    let parsed = parse_action(
        PlanAction::Attach,
        &["plan-1".into(), "--max-live".into(), "6".into()],
    )
    .unwrap();
    assert!(
        parsed
            .runtime
            .validate_against_settings(&settings, None)
            .is_err()
    );
}

#[test]
fn explicit_max_live_honors_task_fleet_and_account_capacity_ceilings() {
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 50,
                max_tasks: 12,
                max_sessions_per_account: 3,
                lanes_per_account: 6,
                fleet_live_cap: Some(8),
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        "config.toml".into(),
        &Default::default(),
    )
    .unwrap();
    let parsed = parse_action(
        PlanAction::RunAll,
        &["plan.json".into(), "--max-live".into(), "9".into()],
    )
    .unwrap();
    assert!(
        parsed
            .runtime
            .validate_against_settings(&settings, None)
            .is_err()
    );

    let parsed = parse_action(
        PlanAction::RunAll,
        &["plan.json".into(), "--max-live".into(), "8".into()],
    )
    .unwrap();
    assert!(
        parsed
            .runtime
            .validate_against_settings(&settings, Some(2))
            .is_err()
    );
    assert!(
        parsed
            .runtime
            .validate_against_settings(&settings, Some(3))
            .is_ok()
    );
}
