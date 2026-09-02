use crate::providers::catalog::{inspect_profile_snapshot, AuthMethod, AuthStatus};
use crate::Engine;

use super::super::fixtures;

#[test]
fn detectors_cover_every_advertised_auth_method_without_retaining_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let claude_oauth = home.join(".claude-oauth");
    let claude_api = home.join(".claude-api");
    let codex_oauth = home.join(".codex-oauth");
    let codex_api = home.join(".codex-api");
    let opencode_oauth = home.join(".opencode-oauth");
    let opencode_api = home.join(".opencode-api");
    let kimi_oauth = home.join(".kimi-oauth");
    let kimi_api = home.join(".kimi-api");
    let grok_oauth = home.join(".grok-oauth");
    let grok_device = home.join(".grok-device");
    let grok_api = home.join(".grok-api");
    let fs = fixtures::FixtureFs::default()
        .file(
            claude_oauth.join(".credentials.json"),
            br#"{"accessToken":"fixture-token","refreshToken":"fixture-token"}"#,
        )
        .file(
            claude_api.join("settings.json"),
            br#"{"env":{"ANTHROPIC_API_KEY":"fixture-token"}}"#,
        )
        .file(
            codex_oauth.join("auth.json"),
            br#"{"tokens":{"access_token":"fixture-token"}}"#,
        )
        .file(
            codex_api.join("auth.json"),
            br#"{"OPENAI_API_KEY":"fixture-token"}"#,
        )
        .file(
            opencode_oauth.join("opencode/auth.json"),
            br#"{"oauth":{"type":"oauth","access":"fixture-token","refresh":"fixture-token"}}"#,
        )
        .file(
            opencode_api.join("opencode/auth.json"),
            br#"{"api":{"type":"api_key","key":"fixture-token"}}"#,
        )
        .file(
            kimi_oauth.join("credentials/kimi-code.json"),
            br#"{"refresh_token":"fixture-token"}"#,
        )
        .file(
            kimi_api.join("config.toml"),
            "default_model = \"kimi-code/k3\"\n[providers.\"managed:kimi-code\"]\ntype = \"kimi\"\napi_key = \"fixture-token\"\n[models.\"kimi-code/k3\"]\nprovider = \"managed:kimi-code\"\nmodel = \"k3\"\nmax_context_size = 1048576\n",
        )
        .file(
            grok_oauth.join("auth.json"),
            br#"{"oidc":{"auth_mode":"oidc","key":"fixture-token"}}"#,
        )
        .file(
            grok_device.join("auth.json"),
            br#"{"device":{"auth_mode":"device","device_token":"fixture-token"}}"#,
        )
        .file(
            grok_api.join("auth.json"),
            br#"{"api":{"auth_mode":"api_key","key":"fixture-token"}}"#,
        );

    let cases = [
        (Engine::Claude, claude_oauth, AuthMethod::OAuth, true),
        (Engine::Claude, claude_api, AuthMethod::ApiKey, false),
        (Engine::Codex, codex_oauth, AuthMethod::OAuth, true),
        (Engine::Codex, codex_api, AuthMethod::ApiKey, false),
        (Engine::Opencode, opencode_oauth, AuthMethod::OAuth, false),
        (Engine::Opencode, opencode_api, AuthMethod::ApiKey, false),
        (Engine::Kimi, kimi_oauth, AuthMethod::OAuth, false),
        (Engine::Kimi, kimi_api, AuthMethod::ApiKey, false),
        (Engine::Grok, grok_oauth, AuthMethod::OAuth, false),
        (Engine::Grok, grok_device, AuthMethod::Device, false),
        (Engine::Grok, grok_api, AuthMethod::ApiKey, false),
    ];
    for (engine, path, expected, rotation) in cases {
        let snapshot = inspect_profile_snapshot(engine, "fixture", path, false, home, &fs);
        let methods = match &snapshot.auth {
            AuthStatus::Authenticated { methods } => methods,
            other => panic!("{engine:?} was not authenticated: {other:?}"),
        };
        assert!(
            methods.contains(&expected),
            "{engine:?} missed {expected:?}"
        );
        assert!(snapshot.eligibility.credential_present);
        assert!(snapshot.eligibility.authenticated);
        assert!(snapshot.eligibility.worker_eligible);
        assert!(snapshot.eligibility.orchestrator_eligible);
        assert!(snapshot.eligibility.managed_pool_eligible);
        assert_eq!(snapshot.eligibility.rotation_eligible, rotation);
        assert!(!format!("{snapshot:?}").contains("fixture-token"));
    }
}

#[test]
fn claude_requires_string_credentials_and_live_expiry() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-whitespace"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-whitespace/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":"   "}}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-number"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-number/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":42}}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-object"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-object/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":{"value":"fixture"}}}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-identity-only"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-identity-only/.claude.json"),
            br#"{"oauthAccount":{"accountUuid":"fixture"}}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-future-field"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-future-field/.credentials.json"),
            br#"{"future":{"accessToken":"fixture"}}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-malformed"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-malformed/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":"fixture"}"#,
        ),
    );
    assert_unauthenticated(
        Engine::Claude,
        home,
        home.join(".claude-expired"),
        fixtures::FixtureFs::default().file(
            home.join(".claude-expired/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":"fixture","refreshToken":"refresh","expiresAt":1}}"#,
        ),
    );

    let live = inspect_profile_snapshot(
        Engine::Claude,
        "live",
        home.join(".claude-live"),
        false,
        home,
        &fixtures::FixtureFs::default().file(
            home.join(".claude-live/.credentials.json"),
            br#"{"claudeAiOauth":{"accessToken":"fixture","expiresAt":4102444800000}}"#,
        ),
    );
    assert_eq!(
        live.auth,
        AuthStatus::Authenticated {
            methods: vec![AuthMethod::OAuth]
        }
    );
}

#[test]
fn opencode_accepts_supported_store_entries_but_not_unknown_or_expired_entries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    let live = inspect_profile_snapshot(
        Engine::Opencode,
        "live",
        home.join(".opencode-live"),
        false,
        home,
        &fixtures::FixtureFs::default().file(
            home.join(".opencode-live/opencode/auth.json"),
            br#"{"anthropic":{"type":"oauth","access":"fixture","expires":"2099-01-01T00:00:00Z"},"future":{"new_field":{"key":"not-a-credential"}}}"#,
        ),
    );
    assert_eq!(
        live.auth,
        AuthStatus::Authenticated {
            methods: vec![AuthMethod::OAuth]
        }
    );

    let api = inspect_profile_snapshot(
        Engine::Opencode,
        "api",
        home.join(".opencode-api"),
        false,
        home,
        &fixtures::FixtureFs::default().file(
            home.join(".opencode-api/opencode/auth.json"),
            br#"{"openai":{"type":"api","key":"fixture"}}"#,
        ),
    );
    assert_eq!(
        api.auth,
        AuthStatus::Authenticated {
            methods: vec![AuthMethod::ApiKey]
        }
    );

    for (name, contents) in [
        (
            "whitespace",
            r#"{"anthropic":{"type":"oauth","access":" ","refresh":""}}"#,
        ),
        ("number", r#"{"anthropic":{"type":"oauth","access":42}}"#),
        (
            "object",
            r#"{"anthropic":{"type":"oauth","access":{"value":"fixture"}}}"#,
        ),
        (
            "expired",
            r#"{"anthropic":{"type":"oauth","access":"fixture","expires_at":1}}"#,
        ),
        (
            "unknown-type",
            r#"{"future":{"type":"future","key":"fixture"}}"#,
        ),
        ("malformed", r#"{"anthropic":{"type":"oauth"}"#),
    ] {
        assert_unauthenticated(
            Engine::Opencode,
            home,
            home.join(format!(".opencode-{name}")),
            fixtures::FixtureFs::default().file(
                home.join(format!(".opencode-{name}/opencode/auth.json")),
                contents,
            ),
        );
    }
}

#[test]
fn grok_requires_declared_supported_auth_and_live_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    let live = inspect_profile_snapshot(
        Engine::Grok,
        "live",
        home.join(".grok-live"),
        false,
        home,
        &fixtures::FixtureFs::default().file(
            home.join(".grok-live/auth.json"),
            br#"{"oidc":{"auth_mode":"oidc","key":"fixture","expiresAt":"2099-01-01T00:00:00Z"},"future":{"new_field":"ignored"}}"#,
        ),
    );
    assert_eq!(
        live.auth,
        AuthStatus::Authenticated {
            methods: vec![AuthMethod::OAuth]
        }
    );

    let api = inspect_profile_snapshot(
        Engine::Grok,
        "api",
        home.join(".grok-api"),
        false,
        home,
        &fixtures::FixtureFs::default().file(
            home.join(".grok-api/auth.json"),
            br#"{"api":{"auth_mode":"api_key","key":"fixture"}}"#,
        ),
    );
    assert_eq!(
        api.auth,
        AuthStatus::Authenticated {
            methods: vec![AuthMethod::ApiKey]
        }
    );

    for (name, contents) in [
        ("whitespace", r#"{"oidc":{"auth_mode":"oidc","key":"   "}}"#),
        ("number", r#"{"oidc":{"auth_mode":"oidc","key":42}}"#),
        (
            "object",
            r#"{"oidc":{"auth_mode":"oidc","key":{"value":"fixture"}}}"#,
        ),
        (
            "expired",
            r#"{"oidc":{"auth_mode":"oidc","key":"fixture","expires_at":1}}"#,
        ),
        (
            "unknown-type",
            r#"{"future":{"auth_mode":"future","key":"fixture"}}"#,
        ),
        ("malformed", r#"{"oidc":{"auth_mode":"oidc"}"#),
    ] {
        assert_unauthenticated(
            Engine::Grok,
            home,
            home.join(format!(".grok-{name}")),
            fixtures::FixtureFs::default()
                .file(home.join(format!(".grok-{name}/auth.json")), contents),
        );
    }
}

fn assert_unauthenticated(
    engine: Engine,
    home: &std::path::Path,
    profile: std::path::PathBuf,
    filesystem: fixtures::FixtureFs,
) {
    let snapshot = inspect_profile_snapshot(engine, "fixture", profile, false, home, &filesystem);
    assert_eq!(snapshot.auth, AuthStatus::Unauthenticated);
    assert!(snapshot.eligibility.credential_present);
    assert!(!snapshot.eligibility.authenticated);
    assert!(!snapshot.eligibility.worker_eligible);
    assert!(!snapshot.eligibility.orchestrator_eligible);
    assert!(!snapshot.eligibility.managed_pool_eligible);
}
