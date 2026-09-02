use std::fs;

use super::super::prepare;

#[test]
fn rejects_inline_api_credentials_without_staging_or_exposing_the_secret() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let state = temp.path().join("state");
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[providers.kimi]\ntype = \"kimi\"\napi_key = \"fixture-api-secret\"\n",
    )
    .unwrap();

    let error = match prepare(&profile, &state) {
        Ok(_) => panic!("inline credential was accepted for Kimi plan staging"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("inline credential"));
    assert!(!message.contains("fixture-api-secret"));
    assert!(fs::read_dir(&state).unwrap().next().is_none());
}

#[test]
fn rejects_an_oversized_profile_config_before_copying_it() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join("profile");
    let state = temp.path().join("state");
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        vec![b'x'; super::super::config::MAX_CONFIG_BYTES + 1],
    )
    .unwrap();

    assert!(prepare(&profile, &state).is_err());
    assert!(state.exists());
}
