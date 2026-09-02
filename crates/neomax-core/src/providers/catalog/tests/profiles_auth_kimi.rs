use crate::providers::catalog::{inspect_profile_snapshot, AuthMethod, AuthStatus};
use crate::Engine;

use super::super::fixtures;

#[test]
fn kimi_api_key_detection_accepts_the_cli_provider_oauth_section_shape() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let profile = home.join(".kimi-code-api");
    let fs = fixtures::FixtureFs::default().file(
        profile.join("config.toml"),
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\napi_key = \"fixture-token\"\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"oauth/kimi-code\"\n",
    );
    let snapshot = inspect_profile_snapshot(Engine::Kimi, "fixture", profile, false, home, &fs);
    assert!(matches!(
        snapshot.auth,
        AuthStatus::Authenticated { ref methods } if methods == &[AuthMethod::ApiKey]
    ));
    assert!(!format!("{snapshot:?}").contains("fixture-token"));
}

#[test]
fn kimi_auth_contract_covers_cli_oauth_and_api_key_fixtures() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    let oauth_profile = home.join(".kimi-oauth");
    assert_kimi_profile(
        "oauth",
        home,
        oauth_profile.clone(),
        fixtures::FixtureFs::default().file(
            oauth_profile.join("credentials/kimi-code.json"),
            br#"{"access_token":"fixture-token","refresh_token":"fixture-refresh"}"#,
        ),
        &[AuthMethod::OAuth],
        true,
    );

    let api_profile = home.join(".kimi-api");
    assert_kimi_profile(
        "provider api key",
        home,
        api_profile.clone(),
        fixtures::FixtureFs::default().file(
            api_profile.join("config.toml"),
            "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\napi_key = \"fixture-token\"\n",
        ),
        &[AuthMethod::ApiKey],
        true,
    );

    let env_api_profile = home.join(".kimi-api-env");
    assert_kimi_profile(
        "provider env api key",
        home,
        env_api_profile.clone(),
        fixtures::FixtureFs::default().file(
            env_api_profile.join("config.toml"),
            "[providers.kimi]\ntype = \"kimi\"\n[providers.kimi.env]\nKIMI_API_KEY = \"fixture-token\"\n",
        ),
        &[AuthMethod::ApiKey],
        true,
    );

    let combined_profile = home.join(".kimi-combined");
    assert_kimi_profile(
        "oauth and provider api key",
        home,
        combined_profile.clone(),
        fixtures::FixtureFs::default()
            .file(
                combined_profile.join("credentials/kimi-code.json"),
                br#"{"refresh_token":"fixture-refresh"}"#,
            )
            .file(
                combined_profile.join("config.toml"),
                "[providers.kimi]\ntype = \"kimi\"\napi_key = \"fixture-token\"\n",
            ),
        &[AuthMethod::OAuth, AuthMethod::ApiKey],
        true,
    );
}

#[test]
fn kimi_auth_contract_fails_closed_for_missing_invalid_and_non_provider_keys() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    assert_kimi_profile(
        "missing",
        home,
        home.join(".kimi-missing"),
        fixtures::FixtureFs::default(),
        &[],
        false,
    );

    let invalid_oauth_profile = home.join(".kimi-invalid-oauth");
    assert_kimi_profile(
        "invalid oauth",
        home,
        invalid_oauth_profile.clone(),
        fixtures::FixtureFs::default().file(
            invalid_oauth_profile.join("credentials/kimi-code.json"),
            br#"{"access_token":"","refresh_token":null}"#,
        ),
        &[],
        true,
    );

    let wrong_type_oauth_profile = home.join(".kimi-wrong-type-oauth");
    assert_kimi_profile(
        "wrong type oauth",
        home,
        wrong_type_oauth_profile.clone(),
        fixtures::FixtureFs::default().file(
            wrong_type_oauth_profile.join("credentials/kimi-code.json"),
            br#"{"access_token":{"value":"fixture-token"}}"#,
        ),
        &[],
        true,
    );

    let invalid_toml_profile = home.join(".kimi-invalid-toml");
    assert_kimi_profile(
        "invalid toml",
        home,
        invalid_toml_profile.clone(),
        fixtures::FixtureFs::default().file(
            invalid_toml_profile.join("config.toml"),
            "[providers.\"managed:kimi-code\"\napi_key = \"fixture-token\"\n",
        ),
        &[],
        true,
    );

    let empty_key_profile = home.join(".kimi-empty-key");
    assert_kimi_profile(
        "empty api key",
        home,
        empty_key_profile.clone(),
        fixtures::FixtureFs::default().file(
            empty_key_profile.join("config.toml"),
            "[providers.kimi]\ntype = \"kimi\"\napi_key = \"   \"\n",
        ),
        &[],
        true,
    );

    let unrelated_env_profile = home.join(".kimi-unrelated-env");
    assert_kimi_profile(
        "unrelated provider environment value",
        home,
        unrelated_env_profile.clone(),
        fixtures::FixtureFs::default().file(
            unrelated_env_profile.join("config.toml"),
            "[providers.kimi]\ntype = \"kimi\"\n[providers.kimi.env]\nNOT_A_CREDENTIAL = \"fixture-token\"\n",
        ),
        &[],
        true,
    );

    let service_key_profile = home.join(".kimi-service-key");
    assert_kimi_profile(
        "service api key",
        home,
        service_key_profile.clone(),
        fixtures::FixtureFs::default().file(
            service_key_profile.join("config.toml"),
            "[services.moonshot_search]\napi_key = \"fixture-token\"\n",
        ),
        &[],
        true,
    );
}

fn assert_kimi_profile(
    name: &str,
    home: &std::path::Path,
    profile: std::path::PathBuf,
    filesystem: fixtures::FixtureFs,
    expected_methods: &[AuthMethod],
    expected_credential_present: bool,
) {
    let snapshot = inspect_profile_snapshot(Engine::Kimi, name, profile, false, home, &filesystem);
    let expected_auth = if expected_methods.is_empty() {
        AuthStatus::Unauthenticated
    } else {
        AuthStatus::Authenticated {
            methods: expected_methods.to_vec(),
        }
    };
    assert_eq!(
        snapshot.auth, expected_auth,
        "unexpected auth state for {name}"
    );
    assert_eq!(
        snapshot.eligibility.credential_present, expected_credential_present,
        "unexpected credential presence for {name}"
    );
    let eligible = !expected_methods.is_empty();
    assert_eq!(snapshot.eligibility.authenticated, eligible, "{name}");
    assert_eq!(snapshot.eligibility.worker_eligible, eligible, "{name}");
    assert_eq!(
        snapshot.eligibility.orchestrator_eligible, eligible,
        "{name}"
    );
    assert_eq!(
        snapshot.eligibility.managed_pool_eligible, eligible,
        "{name}"
    );
    assert!(!format!("{snapshot:?}").contains("fixture-"));
}
