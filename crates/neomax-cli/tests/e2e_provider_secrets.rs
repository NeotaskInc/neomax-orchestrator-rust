#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn env_only_credentials_reach_only_the_selected_provider_process() {
    type CredentialCase = (
        &'static str,
        Engine,
        &'static str,
        &'static [(&'static str, &'static str)],
    );
    let cases: &[CredentialCase] = &[
        (
            "cmax",
            Engine::Claude,
            "secret_anthropic_api_key",
            &[
                ("ANTHROPIC_API_KEY", "fixture-claude-api"),
                ("ANTHROPIC_AUTH_TOKEN", "fixture-claude-auth"),
            ],
        ),
        (
            "cdxmax",
            Engine::Codex,
            "secret_openai_api_key",
            &[
                ("OPENAI_API_KEY", "fixture-codex-openai"),
                ("CODEX_API_KEY", "fixture-codex-key"),
            ],
        ),
        (
            "ocmax",
            Engine::Opencode,
            "secret_opencode_api_key",
            &[
                ("OPENCODE_API_KEY", "fixture-opencode-api"),
                ("OPENCODE_ZEN_API_KEY", "fixture-opencode-zen"),
                ("OPENAI_API_KEY", "fixture-opencode-openai"),
            ],
        ),
        (
            "kmax",
            Engine::Kimi,
            "secret_kimi_api_key",
            &[
                ("KIMI_API_KEY", "fixture-kimi-api"),
                ("KIMI_MODEL_API_KEY", "fixture-kimi-model"),
                ("OPENAI_API_KEY", "fixture-kimi-openai"),
                ("ANTHROPIC_API_KEY", "fixture-kimi-anthropic"),
                ("GOOGLE_API_KEY", "fixture-kimi-google"),
                ("VERTEXAI_API_KEY", "fixture-kimi-vertex"),
            ],
        ),
        (
            "gmax",
            Engine::Grok,
            "secret_xai_api_key",
            &[
                ("XAI_API_KEY", "fixture-grok-xai"),
                ("GROK_API_KEY", "fixture-grok-api"),
                ("GROK_DEPLOYMENT_KEY", "fixture-grok-deployment"),
            ],
        ),
    ];

    for &(launcher, engine, selected_key, selected_keys) in cases {
        let harness = E2eHarness::new([engine]);
        remove_file_backed_credentials(&harness, engine);
        let environment = all_fixture_secrets(selected_keys);
        let args: Vec<&str> = if engine == Engine::Kimi {
            vec!["--json", "--foreground"]
        } else {
            vec!["--json", "--foreground", "fixture env-only task"]
        };
        let result = harness.run_alias_with_env(launcher, args, environment);
        result.assert_success();
        assert!(!result.stdout.contains("fixture-"));
        assert!(!result.stderr.contains("fixture-"));

        let invocation = harness
            .invocations()
            .pop()
            .unwrap_or_else(|| panic!("{launcher} did not invoke its provider"));
        assert_eq!(invocation.field(selected_key), Some("present"));
        for &key in all_secret_field_names() {
            if key != selected_key {
                assert_eq!(invocation.field(key), Some(""), "{launcher} leaked {key}");
            }
        }
        assert!(!format!("{invocation:?}").contains("fixture-"));
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn oauth_profiles_never_receive_ambient_api_keys() {
    for (launcher, engine) in [
        ("cmax", Engine::Claude),
        ("cdxmax", Engine::Codex),
        ("ocmax", Engine::Opencode),
        ("kmax", Engine::Kimi),
        ("gmax", Engine::Grok),
    ] {
        let harness = E2eHarness::new([engine]);
        let args: Vec<&str> = if engine == Engine::Kimi {
            vec!["--json", "--foreground"]
        } else {
            vec!["--json", "--foreground", "fixture oauth task"]
        };
        let result = harness.run_alias_with_env(launcher, args, all_fixture_secrets(&[]));
        result.assert_success();
        assert!(!result.stdout.contains("fixture-"));
        assert!(!result.stderr.contains("fixture-"));
        let invocation = harness
            .invocations()
            .pop()
            .expect("OAuth provider invocation");
        for &key in all_secret_field_names() {
            assert_eq!(invocation.field(key), Some(""), "{launcher} leaked {key}");
        }
        harness.assert_hermetic_invocations();
    }
}

fn all_fixture_secrets(
    selected: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    let mut values = vec![
        ("ANTHROPIC_API_KEY", "fixture-common-anthropic-api"),
        ("ANTHROPIC_AUTH_TOKEN", "fixture-common-anthropic-auth"),
        ("CLAUDE_CODE_OAUTH_TOKEN", "fixture-common-claude-oauth"),
        ("OPENAI_API_KEY", "fixture-common-openai"),
        ("CODEX_API_KEY", "fixture-common-codex"),
        ("OPENCODE_API_KEY", "fixture-common-opencode"),
        ("OPENCODE_ZEN_API_KEY", "fixture-common-opencode-zen"),
        ("KIMI_API_KEY", "fixture-common-kimi"),
        ("KIMI_MODEL_API_KEY", "fixture-common-kimi-model"),
        ("XAI_API_KEY", "fixture-common-xai"),
        ("GROK_API_KEY", "fixture-common-grok"),
        ("GROK_DEPLOYMENT_KEY", "fixture-common-grok-deployment"),
        ("GOOGLE_API_KEY", "fixture-common-google"),
        ("VERTEXAI_API_KEY", "fixture-common-vertex"),
    ];
    for &(key, value) in selected {
        if let Some(entry) = values.iter_mut().find(|entry| entry.0 == key) {
            entry.1 = value;
        }
    }
    values
}

fn all_secret_field_names() -> &'static [&'static str] {
    &[
        "secret_anthropic_api_key",
        "secret_anthropic_auth_token",
        "secret_claude_code_oauth_token",
        "secret_openai_api_key",
        "secret_codex_api_key",
        "secret_opencode_api_key",
        "secret_opencode_zen_api_key",
        "secret_kimi_api_key",
        "secret_kimi_model_api_key",
        "secret_xai_api_key",
        "secret_grok_api_key",
        "secret_grok_deployment_key",
        "secret_google_api_key",
        "secret_vertexai_api_key",
    ]
}

fn remove_file_backed_credentials(harness: &E2eHarness, engine: Engine) {
    let profile = harness.profile(engine, 0);
    let paths = match engine {
        Engine::Claude => vec![
            profile.join(".credentials.json"),
            profile.join(".claude.json"),
        ],
        Engine::Codex | Engine::Grok => vec![profile.join("auth.json")],
        Engine::Opencode => vec![profile.join("opencode/auth.json")],
        Engine::Kimi => vec![profile.join("credentials/kimi-code.json")],
    };
    for path in paths {
        fs::remove_file(path).expect("remove seeded file credential");
    }
}
