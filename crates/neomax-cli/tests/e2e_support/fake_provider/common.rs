use neomax_core::Engine;

#[cfg(unix)]
pub(super) const SECRET_ENV_NAMES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "OPENCODE_API_KEY",
    "OPENCODE_ZEN_API_KEY",
    "KIMI_API_KEY",
    "KIMI_MODEL_API_KEY",
    "XAI_API_KEY",
    "GROK_API_KEY",
    "GROK_DEPLOYMENT_KEY",
    "GOOGLE_API_KEY",
    "VERTEXAI_API_KEY",
];

pub(super) fn provider_name(engine: Engine) -> &'static str {
    engine.as_str()
}

#[cfg(unix)]
pub(super) fn profile_env(provider: &str) -> &'static str {
    match provider {
        "claude" => "CLAUDE_CONFIG_DIR",
        "codex" => "CODEX_HOME",
        "opencode" => "XDG_DATA_HOME",
        "kimi" => "KIMI_CODE_HOME",
        "grok" => "GROK_HOME",
        _ => "",
    }
}
