use super::super::{
    all_specs, default_model_id, spec, supports_native_interactive_resume, supports_native_resume,
    AuthMethod, ModelDiscoverySupport, CODEX_SERVICE_TIER,
};
use crate::Engine;

#[test]
fn every_provider_spec_has_a_complete_runtime_contract() {
    let specs = all_specs().collect::<Vec<_>>();
    assert_eq!(specs.len(), Engine::ALL.len());
    for provider in specs {
        assert_eq!(provider.default_model, default_model_id(provider.engine));
        assert!(!provider.default_binary.is_empty());
        assert!(!provider.config_env.is_empty());
        assert!(!provider.profile_env.is_empty());
        assert!(!provider.account_prefix.is_empty());
        assert!(provider.capabilities.orchestrator);
        assert!(provider.capabilities.worker);
    }
    assert_eq!(spec(Engine::Codex).default_model, "gpt-5.6-sol");
    assert_eq!(CODEX_SERVICE_TIER, "fast");
}

#[test]
fn model_listing_is_marked_best_effort_only_where_local_commands_exist() {
    assert_eq!(
        spec(Engine::Claude).capabilities.model_discovery,
        ModelDiscoverySupport::Unavailable
    );
    assert_eq!(
        spec(Engine::Codex).capabilities.model_discovery,
        ModelDiscoverySupport::Unavailable
    );
    for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
        assert_eq!(
            spec(engine).capabilities.model_discovery,
            ModelDiscoverySupport::BestEffort
        );
    }
}

#[test]
fn opencode_auth_supports_api_key_and_oauth_and_scrubs_auth_content() {
    let provider = spec(Engine::Opencode);
    assert_eq!(
        provider.capabilities.auth_methods.as_slice(),
        &[AuthMethod::ApiKey, AuthMethod::OAuth]
    );
    assert!(provider
        .scrub
        .iter()
        .any(|key| key == "OPENCODE_AUTH_CONTENT"));
}

#[test]
fn codex_metadata_exposes_device_auth_alongside_oauth_and_api_key() {
    assert_eq!(
        spec(Engine::Codex).capabilities.auth_methods.as_slice(),
        &[AuthMethod::OAuth, AuthMethod::Device, AuthMethod::ApiKey]
    );
}

#[test]
fn codex_durable_resume_starts_a_fresh_thread() {
    assert!(!supports_native_resume(Engine::Codex));
    for engine in [Engine::Claude, Engine::Opencode, Engine::Kimi, Engine::Grok] {
        assert!(supports_native_resume(engine));
    }
}

#[test]
fn every_provider_supports_native_interactive_resume() {
    for engine in Engine::ALL {
        assert!(supports_native_interactive_resume(engine));
    }
}
