use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{
    ConcurrencySettings, EffectiveSettings, FLEET_CAP_ENV, LANES_PER_ACCOUNT_ENV,
    LEGACY_AGENT_BUDGET_ENV, LEGACY_FLEET_CAP_ENV, LEGACY_LANES_PER_ACCT_ENV, LEGACY_QUEUE_TTL_ENV,
    LEGACY_TASK_BUDGET_ENV, MAX_LIVE_ENV, MAX_SESSIONS_PER_ACCOUNT_ENV, MAX_SUBAGENTS_ENV,
    MAX_TASKS_ENV, QUEUE_TTL_SECONDS_ENV, SettingsFile,
};

fn resolve(environment: BTreeMap<String, String>) -> EffectiveSettings {
    EffectiveSettings::resolve(
        SettingsFile::default(),
        PathBuf::from("config.toml"),
        &environment,
    )
    .unwrap()
}

#[test]
fn canonical_subagent_limit_overrides_legacy_aliases() {
    let effective = resolve(BTreeMap::from([
        (LEGACY_FLEET_CAP_ENV.into(), "10".into()),
        (LEGACY_AGENT_BUDGET_ENV.into(), "20".into()),
        (MAX_SUBAGENTS_ENV.into(), "30".into()),
    ]));
    assert_eq!(effective.concurrency.max_subagents, 30);
    assert_eq!(effective.max_subagents_source, MAX_SUBAGENTS_ENV);
}

#[test]
fn fleet_cap_never_overrides_subagent_budget() {
    let effective = resolve(BTreeMap::from([
        (FLEET_CAP_ENV.into(), "19".into()),
        (MAX_LIVE_ENV.into(), "0".into()),
    ]));
    assert_eq!(effective.concurrency.max_subagents, 50);
    assert_eq!(effective.concurrency.fleet_live_cap, Some(0));
    assert_eq!(effective.max_subagents_source, "config.toml");
}

#[test]
fn canonical_concurrency_names_override_legacy_aliases() {
    let effective = resolve(BTreeMap::from([
        (LEGACY_TASK_BUDGET_ENV.into(), "2".into()),
        (MAX_TASKS_ENV.into(), "4".into()),
        (LEGACY_LANES_PER_ACCT_ENV.into(), "3".into()),
        (LANES_PER_ACCOUNT_ENV.into(), "5".into()),
        (LEGACY_QUEUE_TTL_ENV.into(), "30".into()),
        (QUEUE_TTL_SECONDS_ENV.into(), "60".into()),
    ]));
    assert_eq!(effective.concurrency.max_tasks, 4);
    assert_eq!(effective.concurrency.lanes_per_account, 5);
    assert_eq!(effective.concurrency.queue_ttl_seconds, 60.0);
}

#[test]
fn canonical_names_are_not_poisoned_by_invalid_legacy_values() {
    let effective = resolve(BTreeMap::from([
        (LEGACY_AGENT_BUDGET_ENV.into(), "not-a-number".into()),
        (MAX_SUBAGENTS_ENV.into(), "30".into()),
        (LEGACY_TASK_BUDGET_ENV.into(), "not-a-number".into()),
        (MAX_TASKS_ENV.into(), "4".into()),
        (LEGACY_LANES_PER_ACCT_ENV.into(), "not-a-number".into()),
        (LANES_PER_ACCOUNT_ENV.into(), "5".into()),
        (LEGACY_QUEUE_TTL_ENV.into(), "not-a-number".into()),
        (QUEUE_TTL_SECONDS_ENV.into(), "60".into()),
    ]));
    assert_eq!(effective.concurrency.max_subagents, 30);
    assert_eq!(effective.concurrency.max_tasks, 4);
    assert_eq!(effective.concurrency.lanes_per_account, 5);
    assert_eq!(effective.concurrency.queue_ttl_seconds, 60.0);
}

#[test]
fn propagated_session_limit_overrides_the_persistent_value() {
    let effective = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_sessions_per_account: 10,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        PathBuf::from("config.toml"),
        &BTreeMap::from([(MAX_SESSIONS_PER_ACCOUNT_ENV.into(), "4".into())]),
    )
    .unwrap();
    assert_eq!(effective.concurrency.max_sessions_per_account, 4);
    assert_eq!(
        effective
            .agent_environment()
            .get(MAX_SESSIONS_PER_ACCOUNT_ENV),
        Some(&"4".to_string())
    );
}

#[test]
fn propagated_canonical_session_limit_wins_over_legacy_live_cap_when_both_are_present() {
    let effective = EffectiveSettings::resolve(
        SettingsFile::default(),
        PathBuf::from("config.toml"),
        &BTreeMap::from([
            (MAX_SESSIONS_PER_ACCOUNT_ENV.into(), "4".into()),
            ("NEOMAX_LIVE_CAP".into(), "7".into()),
        ]),
    )
    .unwrap();
    assert_eq!(effective.concurrency.max_sessions_per_account, 4);
}

#[test]
fn invalid_effective_environment_values_are_rejected() {
    for (key, value) in [
        (MAX_SUBAGENTS_ENV, "0"),
        (MAX_SESSIONS_PER_ACCOUNT_ENV, "0"),
        ("NEOMAX_LIVE_CAP", "0"),
        (MAX_LIVE_ENV, "-1"),
        (MAX_TASKS_ENV, "-1"),
        (LANES_PER_ACCOUNT_ENV, "0"),
        (QUEUE_TTL_SECONDS_ENV, "NaN"),
    ] {
        assert!(
            EffectiveSettings::resolve(
                SettingsFile::default(),
                PathBuf::from("config.toml"),
                &BTreeMap::from([(key.into(), value.into())]),
            )
            .is_err(),
            "{key}={value} should be rejected"
        );
    }
}

#[test]
fn child_agents_receive_the_effective_limit_set_in_one_place() {
    let effective = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 37,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        PathBuf::from("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap();
    let child = effective.agent_environment();
    assert_eq!(child.get(MAX_SUBAGENTS_ENV), Some(&"37".to_string()));
    assert_eq!(child.get(MAX_TASKS_ENV), Some(&"0".to_string()));
    assert_eq!(
        child.get(MAX_SESSIONS_PER_ACCOUNT_ENV),
        Some(&"10".to_string())
    );
    assert_eq!(child.get(LANES_PER_ACCOUNT_ENV), Some(&"6".to_string()));
    assert_eq!(child.get(QUEUE_TTL_SECONDS_ENV), Some(&"43200".to_string()));
    assert_eq!(child.get(MAX_LIVE_ENV), Some(&"50".to_string()));
}
