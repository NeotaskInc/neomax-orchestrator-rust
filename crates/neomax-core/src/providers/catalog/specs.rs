use crate::Engine;

use super::types::{AuthMethod, ModelDiscoverySupport, ProviderCapabilities, ProviderSpec};

pub const CLAUDE_DEFAULT_MODEL: &str = "claude-fable-5[1m]";
pub const CLAUDE_OPUS_MODEL: &str = "claude-opus-5";
pub const CLAUDE_OPUS_MODEL_1M: &str = "claude-opus-5[1m]";
pub const CODEX_DEFAULT_MODEL: &str = "gpt-5.6-sol";
pub const CODEX_SERVICE_TIER: &str = "fast";
pub const OPENCODE_DEFAULT_MODEL: &str = "opencode/big-pickle";
pub const KIMI_DEFAULT_MODEL: &str = "kimi-code/k3";
pub const GROK_DEFAULT_MODEL: &str = "grok-4.6";

const CLAUDE_AUTH: &[AuthMethod] = &[AuthMethod::OAuth, AuthMethod::ApiKey];
const CODEX_AUTH: &[AuthMethod] = &[AuthMethod::OAuth, AuthMethod::Device, AuthMethod::ApiKey];
const OPENCODE_AUTH: &[AuthMethod] = &[AuthMethod::ApiKey, AuthMethod::OAuth];
const KIMI_AUTH: &[AuthMethod] = &[AuthMethod::OAuth, AuthMethod::ApiKey];
const GROK_AUTH: &[AuthMethod] = &[AuthMethod::OAuth, AuthMethod::Device, AuthMethod::ApiKey];

fn capabilities(
    auth_methods: &[AuthMethod],
    model_discovery: ModelDiscoverySupport,
) -> ProviderCapabilities {
    ProviderCapabilities {
        orchestrator: true,
        worker: true,
        multiple_profiles: true,
        model_discovery,
        native_sessions: true,
        usage_discovery: true,
        auth_methods: auth_methods.to_vec(),
    }
}

pub fn spec(engine: Engine) -> ProviderSpec {
    match engine {
        Engine::Claude => ProviderSpec {
            engine,
            default_binary: "claude".into(),
            binary_env: "NEOMAX_CLAUDE_BIN".into(),
            config_env: "CLAUDE_CONFIG_DIR".into(),
            profile_env: "NEOMAX_PROFILES".into(),
            default_profile_dir: ".claude".into(),
            account_prefix: ".claude-acct".into(),
            orchestrator_dir: ".claude-orch".into(),
            orchestrator_env: "NEOMAX_CLAUDE_ORCH".into(),
            model_env: "NEOMAX_CLAUDE_MODEL".into(),
            default_model: CLAUDE_DEFAULT_MODEL.into(),
            model_args: Vec::new(),
            default_unsets_config_env: true,
            scrub: vec![
                "ANTHROPIC_API_KEY".into(),
                "ANTHROPIC_AUTH_TOKEN".into(),
                "CLAUDE_CODE_OAUTH_TOKEN".into(),
            ],
            capabilities: capabilities(CLAUDE_AUTH, ModelDiscoverySupport::Unavailable),
        },
        Engine::Codex => ProviderSpec {
            engine,
            default_binary: "codex".into(),
            binary_env: "NEOMAX_CODEX_BIN".into(),
            config_env: "CODEX_HOME".into(),
            profile_env: "NEOMAX_CODEX_PROFILES".into(),
            default_profile_dir: ".codex".into(),
            account_prefix: ".codex-acct".into(),
            orchestrator_dir: ".codex-orch".into(),
            orchestrator_env: "NEOMAX_CODEX_ORCH".into(),
            model_env: "NEOMAX_CODEX_MODEL".into(),
            default_model: CODEX_DEFAULT_MODEL.into(),
            model_args: Vec::new(),
            default_unsets_config_env: false,
            scrub: vec!["OPENAI_API_KEY".into(), "CODEX_API_KEY".into()],
            capabilities: capabilities(CODEX_AUTH, ModelDiscoverySupport::Unavailable),
        },
        Engine::Opencode => ProviderSpec {
            engine,
            default_binary: "opencode".into(),
            binary_env: "NEOMAX_OPENCODE_BIN".into(),
            config_env: "XDG_DATA_HOME".into(),
            profile_env: "NEOMAX_OPENCODE_PROFILES".into(),
            default_profile_dir: ".opencode".into(),
            account_prefix: ".opencode-acct".into(),
            orchestrator_dir: ".opencode-orch".into(),
            orchestrator_env: "NEOMAX_OPENCODE_ORCH".into(),
            model_env: "NEOMAX_OPENCODE_MODEL".into(),
            default_model: OPENCODE_DEFAULT_MODEL.into(),
            model_args: vec!["models".into()],
            default_unsets_config_env: true,
            scrub: vec![
                "OPENCODE_API_KEY".into(),
                "OPENCODE_ZEN_API_KEY".into(),
                "OPENCODE_AUTH_CONTENT".into(),
                "OPENAI_API_KEY".into(),
            ],
            capabilities: capabilities(OPENCODE_AUTH, ModelDiscoverySupport::BestEffort),
        },
        Engine::Kimi => ProviderSpec {
            engine,
            default_binary: "kimi".into(),
            binary_env: "NEOMAX_KIMI_BIN".into(),
            config_env: "KIMI_CODE_HOME".into(),
            profile_env: "NEOMAX_KIMI_PROFILES".into(),
            default_profile_dir: ".kimi-code".into(),
            account_prefix: ".kimi-code-acct".into(),
            orchestrator_dir: ".kimi-code-orch".into(),
            orchestrator_env: "NEOMAX_KIMI_ORCH".into(),
            model_env: "NEOMAX_KIMI_MODEL".into(),
            default_model: KIMI_DEFAULT_MODEL.into(),
            model_args: vec!["provider".into(), "list".into(), "--json".into()],
            default_unsets_config_env: true,
            scrub: vec![
                "KIMI_API_KEY".into(),
                "KIMI_MODEL_API_KEY".into(),
                "KIMI_MODEL_BASE_URL".into(),
                "KIMI_MODEL_NAME".into(),
                "KIMI_CODE_BASE_URL".into(),
                "KIMI_BASE_URL".into(),
                "OPENAI_API_KEY".into(),
                "ANTHROPIC_API_KEY".into(),
            ],
            capabilities: capabilities(KIMI_AUTH, ModelDiscoverySupport::BestEffort),
        },
        Engine::Grok => ProviderSpec {
            engine,
            default_binary: "grok".into(),
            binary_env: "NEOMAX_GROK_BIN".into(),
            config_env: "GROK_HOME".into(),
            profile_env: "NEOMAX_GROK_PROFILES".into(),
            default_profile_dir: ".grok".into(),
            account_prefix: ".grok-acct".into(),
            orchestrator_dir: ".grok-orch".into(),
            orchestrator_env: "NEOMAX_GROK_ORCH".into(),
            model_env: "NEOMAX_GROK_MODEL".into(),
            default_model: GROK_DEFAULT_MODEL.into(),
            model_args: vec!["models".into()],
            default_unsets_config_env: true,
            scrub: vec![
                "NEOMAX_GROK_API_KEY".into(),
                "XAI_API_KEY".into(),
                "GROK_API_KEY".into(),
                "GROK_DEPLOYMENT_KEY".into(),
                "OPENAI_API_KEY".into(),
                "ANTHROPIC_API_KEY".into(),
            ],
            capabilities: capabilities(GROK_AUTH, ModelDiscoverySupport::BestEffort),
        },
    }
}

pub fn all_specs() -> impl Iterator<Item = ProviderSpec> {
    Engine::ALL.into_iter().map(spec)
}

pub fn default_model_id(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => CLAUDE_DEFAULT_MODEL,
        Engine::Codex => CODEX_DEFAULT_MODEL,
        Engine::Opencode => OPENCODE_DEFAULT_MODEL,
        Engine::Kimi => KIMI_DEFAULT_MODEL,
        Engine::Grok => GROK_DEFAULT_MODEL,
    }
}

pub const fn supports_native_resume(engine: Engine) -> bool {
    !matches!(engine, Engine::Codex)
}

pub const fn supports_native_interactive_resume(_engine: Engine) -> bool {
    true
}
