use std::fs;
use std::path::Path;

use neomax_core::Engine;

pub(super) fn profile_stem(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "claude",
        Engine::Codex => "codex",
        Engine::Opencode => "opencode",
        Engine::Kimi => "kimi-code",
        Engine::Grok => "grok",
    }
}

pub(super) fn profile_env(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "NEOMAX_PROFILES",
        Engine::Codex => "NEOMAX_CODEX_PROFILES",
        Engine::Opencode => "NEOMAX_OPENCODE_PROFILES",
        Engine::Kimi => "NEOMAX_KIMI_PROFILES",
        Engine::Grok => "NEOMAX_GROK_PROFILES",
    }
}

pub(super) fn binary_env(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "NEOMAX_CLAUDE_BIN",
        Engine::Codex => "NEOMAX_CODEX_BIN",
        Engine::Opencode => "NEOMAX_OPENCODE_BIN",
        Engine::Kimi => "NEOMAX_KIMI_BIN",
        Engine::Grok => "NEOMAX_GROK_BIN",
    }
}

pub(super) fn seed_profile(engine: Engine, profile: &Path) {
    fs::create_dir_all(profile).expect("profile directory");
    match engine {
        Engine::Claude => {
            let account = profile
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("fixture");
            fs::write(
                profile.join(".credentials.json"),
                r#"{"claudeAiOauth":{"accessToken":"fixture-token","refreshToken":"fixture-refresh"}}"#,
            )
            .expect("Claude credential fixture");
            fs::write(
                profile.join(".claude.json"),
                format!(
                    r#"{{"oauthAccount":{{"accountUuid":"fixture-{account}","emailAddress":"{account}@example.test"}}}}"#
                ),
            )
        }
        Engine::Codex => fs::write(
            profile.join("auth.json"),
            r#"{"tokens":{"access_token":"fixture"}}"#,
        ),
        Engine::Opencode => {
            fs::create_dir_all(profile.join("opencode")).expect("opencode profile");
            fs::write(
                profile.join("opencode/auth.json"),
                r#"{"fixture":{"refresh_token":"fixture"}}"#,
            )
        }
        Engine::Kimi => {
            fs::create_dir_all(profile.join("credentials")).expect("kimi profile");
            fs::write(
                profile.join("credentials/kimi-code.json"),
                r#"{"refresh_token":"fixture"}"#,
            )
        }
        Engine::Grok => fs::write(
            profile.join("auth.json"),
            r#"{"xai::oidc":{"refresh_token":"fixture"}}"#,
        ),
    }
    .expect("profile credential");
}
