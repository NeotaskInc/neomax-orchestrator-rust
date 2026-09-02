use super::{ConcurrencySettings, SettingsFile};

#[test]
fn config_round_trips_atomically_and_preserves_future_fields() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("neomax/config.toml");
    let settings = SettingsFile::default();
    settings.save(&path).unwrap();
    assert_eq!(SettingsFile::load(&path).unwrap(), settings);

    std::fs::write(
        &path,
        "future_root = 'kept'\n[concurrency]\nmax_subagents = 12\nfuture_limit = 9\n",
    )
    .unwrap();
    let mut loaded = SettingsFile::load(&path).unwrap();
    loaded.concurrency.max_subagents = 20;
    loaded.save(&path).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("future_root = \"kept\""));
    assert!(saved.contains("future_limit = 9"));
}

#[test]
fn missing_config_uses_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let settings = SettingsFile::load(&temp.path().join("missing.toml")).unwrap();
    assert_eq!(settings, SettingsFile::default());
}

#[test]
fn invalid_persistent_concurrency_is_rejected_before_write() {
    let temp = tempfile::tempdir().unwrap();
    let settings = SettingsFile {
        concurrency: ConcurrencySettings {
            max_subagents: 0,
            ..ConcurrencySettings::default()
        },
        ..SettingsFile::default()
    };
    assert!(settings.save(&temp.path().join("config.toml")).is_err());
}

#[test]
fn path_uses_xdg_override_or_platform_default() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("neomax-home");
    let config = temp.path().join("config");
    assert_eq!(
        SettingsFile::path(&home, Some(config.as_path())),
        config.join("neomax/config.toml")
    );
    assert_eq!(
        SettingsFile::path(&home, None),
        home.join(".config/neomax/config.toml")
    );
}
