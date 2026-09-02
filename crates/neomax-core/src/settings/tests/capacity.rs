use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{ConcurrencySettings, EffectiveSettings, MAX_LIVE_ENV, SettingsFile};

#[test]
fn default_capacity_uses_accounts_lanes_and_effective_ceilings() {
    let settings = EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 50,
                max_tasks: 9,
                max_sessions_per_account: 3,
                lanes_per_account: 6,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        PathBuf::from("config.toml"),
        &BTreeMap::from([(MAX_LIVE_ENV.into(), "7".into())]),
    )
    .unwrap();
    assert_eq!(settings.default_run_all_capacity(4), 7);
    assert_eq!(settings.default_run_all_capacity(1), 3);
    assert_eq!(settings.default_run_all_capacity(0), 0);
}

#[test]
fn explicit_capacity_rejects_requests_above_the_effective_limit() {
    let settings = EffectiveSettings::resolve(
        SettingsFile::default(),
        PathBuf::from("config.toml"),
        &BTreeMap::from([(MAX_LIVE_ENV.into(), "2".into())]),
    )
    .unwrap();
    settings.validate_run_all_capacity(2, 1).unwrap();
    let error = settings.validate_run_all_capacity(3, 1).unwrap_err();
    assert!(error.to_string().contains("effective run-all capacity 2"));
}
