use neomax_core::SettingsFile;

use crate::config;
use crate::tests::fixture;

#[test]
fn max_subagents_is_saved_in_the_single_user_config_file() {
    let fixture = fixture();
    config::run(
        &fixture.context,
        &["set".into(), "max-subagents".into(), "9".into()],
    )
    .expect("set max-subagents");
    let saved = SettingsFile::load(&fixture.context.settings.config_path).expect("saved config");
    assert_eq!(saved.concurrency.max_subagents, 9);
}

#[test]
fn zero_max_subagents_is_rejected() {
    let fixture = fixture();
    let error = config::run(
        &fixture.context,
        &["set".into(), "max-subagents".into(), "0".into()],
    )
    .expect_err("zero max-subagents should fail");
    assert!(error.to_string().contains("positive integer"));
}

#[test]
fn max_sessions_per_account_is_saved_in_the_single_user_config_file() {
    let fixture = fixture();
    config::run(
        &fixture.context,
        &["set".into(), "max-sessions-per-account".into(), "7".into()],
    )
    .expect("set max-sessions-per-account");
    let saved = SettingsFile::load(&fixture.context.settings.config_path).expect("saved config");
    assert_eq!(saved.concurrency.max_sessions_per_account, 7);
}

#[test]
fn zero_max_sessions_per_account_is_rejected() {
    let fixture = fixture();
    let error = config::run(
        &fixture.context,
        &["set".into(), "max-sessions-per-account".into(), "0".into()],
    )
    .expect_err("zero max-sessions-per-account should fail");
    assert!(error.to_string().contains("positive integer"));
}
