use std::fs;

use std::collections::BTreeMap;

use anyhow::Result;
use neomax_core::Engine;

use super::{
    GROK_API_KEY_ENVIRONMENT, api_key_from_values, choose_auth_mode, configure_grok_api_key,
    configure_kimi_api_key, set_preferred_auth,
};
use crate::operations::account_helpers::prompt::PromptPort;
use crate::operations::account_helpers::request::AuthMode;

struct FixturePrompt {
    selection: String,
    secret: String,
}

impl PromptPort for FixturePrompt {
    fn selection(&self, _prompt: &str) -> Result<String> {
        Ok(self.selection.clone())
    }

    fn secret(&self, _prompt: &str) -> Result<String> {
        Ok(self.secret.clone())
    }
}

#[test]
fn grok_api_key_aliases_use_canonical_precedence_without_rendering_values() {
    let values = BTreeMap::from([
        ("XAI_API_KEY", "xai-secret"),
        ("GROK_API_KEY", "grok-secret"),
        ("GROK_DEPLOYMENT_KEY", "deployment-secret"),
        ("NEOMAX_GROK_API_KEY", "canonical-secret"),
    ]);
    let key = api_key_from_values(Engine::Grok, |name| {
        values.get(name).map(|value| (*value).to_owned())
    })
    .unwrap();
    assert_eq!(key, "canonical-secret");
    assert_eq!(GROK_API_KEY_ENVIRONMENT[0], "NEOMAX_GROK_API_KEY");
    assert!(!format!("{key:?}").contains("xai-secret"));
}

#[test]
fn grok_choose_resolves_all_supported_auth_methods_through_injected_input() {
    for (selection, expected) in [
        ("", AuthMode::OAuth),
        ("1", AuthMode::OAuth),
        ("2", AuthMode::Device),
        ("3", AuthMode::ApiKey),
        ("api-key", AuthMode::ApiKey),
    ] {
        let prompt = FixturePrompt {
            selection: selection.into(),
            secret: "fixture-secret".into(),
        };
        assert_eq!(choose_auth_mode(Engine::Grok, &prompt).unwrap(), expected);
    }
}

#[test]
fn grok_preferred_auth_is_persisted_without_credentials_in_config() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".grok-acct2");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("config.toml"),
        "preferred_method = \"oauth\"\n",
    )
    .unwrap();

    set_preferred_auth(Engine::Grok, &profile, AuthMode::Device).unwrap();
    let config = std::fs::read_to_string(profile.join("config.toml")).unwrap();
    assert!(config.contains("preferred_method = \"oidc\""));
    assert!(!config.contains("fixture-secret"));

    set_preferred_auth(Engine::Grok, &profile, AuthMode::ApiKey).unwrap();
    let config = std::fs::read_to_string(profile.join("config.toml")).unwrap();
    assert!(config.contains("preferred_method = \"api_key\""));
    assert!(!config.contains("fixture-secret"));

    #[cfg(windows)]
    neomax_core::io::verify_private_path(&profile.join("config.toml")).unwrap();
}

#[test]
fn grok_api_key_configuration_is_private_and_keeps_secret_out_of_paths() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".grok-acct2");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("auth.json"), "{\"existing\":true}\n").unwrap();
    fs::write(profile.join("config.toml"), "preferred_method = \"oidc\"\n").unwrap();

    configure_grok_api_key(&profile, "fixture-secret").unwrap();

    let auth_path = profile.join("auth.json");
    let auth = fs::read_to_string(&auth_path).unwrap();
    assert!(auth.contains("fixture-secret"));
    assert!(auth.contains("xai::api_key"));
    assert!(!auth_path.to_string_lossy().contains("fixture-secret"));
    assert!(
        fs::read_to_string(profile.join("config.toml"))
            .unwrap()
            .contains("preferred_method = \"api_key\"")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&auth_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[cfg(windows)]
    {
        neomax_core::io::verify_private_path(&auth_path).unwrap();
        neomax_core::io::verify_private_path(&profile.join("config.toml")).unwrap();
        neomax_core::io::verify_private_path(&profile).unwrap();
    }
}

#[test]
fn kimi_api_key_configuration_writes_provider_and_model_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path().join(".kimi-code-acct2");
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        profile.join("config.toml"),
        "[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\napi_key = \"\"\nbase_url = \"https://api.kimi.com/coding/v1\"\n\n[providers.\"managed:kimi-code\".oauth]\nstorage = \"file\"\nkey = \"access\"\n",
    )
    .unwrap();

    configure_kimi_api_key(&profile, "fixture-api-key").unwrap();

    let config = fs::read_to_string(profile.join("config.toml")).unwrap();
    for expected in [
        "default_model = \"kimi-code/k3\"",
        "[providers.\"managed:kimi-code\"]",
        "type = \"kimi\"",
        "base_url = \"https://api.kimi.com/coding/v1\"",
        "api_key = \"fixture-api-key\"",
        "[providers.\"managed:kimi-code\".oauth]",
        "storage = \"file\"",
        "key = \"access\"",
        "[models.\"kimi-code/k3\"]",
        "provider = \"managed:kimi-code\"",
        "model = \"k3\"",
        "[models.\"kimi-code/kimi-for-coding\"]",
    ] {
        assert!(config.contains(expected), "missing {expected} in {config}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(profile.join("config.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&profile).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[cfg(windows)]
    neomax_core::io::verify_private_path(&profile.join("config.toml")).unwrap();
}
