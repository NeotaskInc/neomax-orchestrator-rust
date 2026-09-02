use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::*;

fn request(
    engine: Engine,
    operation: AccountOperation,
    model: Option<&str>,
) -> AccountHelperRequest {
    AccountHelperRequest {
        engine,
        account: super::super::request::AccountSelector::Number(2),
        operation,
        model: model.map(str::to_owned),
        forwarded: vec!["--prompt".into()],
        json: false,
    }
}

fn login_request(
    engine: Engine,
    auth_mode: AuthMode,
    region: Option<&str>,
) -> AccountHelperRequest {
    AccountHelperRequest {
        engine,
        account: super::super::request::AccountSelector::Number(2),
        operation: AccountOperation::Login {
            auth_mode,
            provider: None,
            region: region.map(str::to_owned),
        },
        model: None,
        forwarded: Vec::new(),
        json: false,
    }
}

fn profile(engine: Engine, auth: Option<DetectedAuth>) -> ManagedProfile {
    ManagedProfile {
        profile: neomax_core::providers::ProviderProfile {
            engine,
            account: "2".into(),
            path: PathBuf::from("/fixture/profile"),
            reserved: false,
        },
        auth,
    }
}

#[test]
fn kimi_oauth_and_api_key_runs_forward_models_correctly() {
    let oauth = run_command(
        &request(Engine::Kimi, AccountOperation::Run, None),
        &profile(Engine::Kimi, Some(DetectedAuth::OAuth)),
        "kimi-code/k3",
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert!(
        oauth
            .args
            .windows(2)
            .any(|pair| { pair == [OsString::from("-m"), OsString::from("kimi-code/k3")] })
    );

    let api_key = run_command(
        &request(Engine::Kimi, AccountOperation::Run, None),
        &profile(Engine::Kimi, Some(DetectedAuth::ApiKey)),
        "kimi-code/k3",
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert!(!api_key.args.contains(&"-m".into()));

    let explicit = run_command(
        &request(Engine::Kimi, AccountOperation::Run, Some("kimi-code/k2.7")),
        &profile(Engine::Kimi, Some(DetectedAuth::ApiKey)),
        "kimi-code/k2.7",
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert!(explicit.args.contains(&"kimi-code/k2.7".into()));
}

#[test]
fn grok_login_modes_map_to_distinct_provider_flows() {
    for (mode, expected) in [
        (AuthMode::OAuth, vec!["login", "--oauth"]),
        (AuthMode::Device, vec!["login", "--device-auth"]),
    ] {
        let invocation = login_command(
            &request(
                Engine::Grok,
                AccountOperation::Login {
                    auth_mode: mode,
                    provider: None,
                    region: None,
                },
                None,
            ),
            &profile(Engine::Grok, None),
            Path::new("/fixture/home"),
            Path::new("/fixture/workspace"),
        )
        .unwrap();
        assert!(invocation.interactive);
        assert_eq!(&invocation.args[..2], expected);
    }
}

#[test]
fn grok_choose_cannot_fall_through_to_implicit_oauth() {
    let result = login_command(
        &login_request(Engine::Grok, AuthMode::Choose, None),
        &profile(Engine::Grok, None),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    );
    let error = result.expect_err("unresolved Grok selection must not invoke OAuth");
    assert!(error.to_string().contains("selection must be resolved"));
}

#[test]
fn codex_login_modes_use_supported_cli_flags() {
    for (mode, expected) in [
        (AuthMode::Device, vec!["login", "--device-auth"]),
        (AuthMode::ApiKey, vec!["login", "--with-api-key"]),
        (AuthMode::AccessToken, vec!["login", "--with-access-token"]),
    ] {
        let invocation = login_command(
            &request(
                Engine::Codex,
                AccountOperation::Login {
                    auth_mode: mode,
                    provider: None,
                    region: None,
                },
                None,
            ),
            &profile(Engine::Codex, None),
            Path::new("/fixture/home"),
            Path::new("/fixture/workspace"),
        )
        .unwrap();
        assert_eq!(&invocation.args[..2], expected);
        assert!(
            !invocation
                .args
                .iter()
                .any(|value| value.to_string_lossy().contains("fixture")),
            "credential values must never be placed on Codex login argv"
        );
    }
}

#[test]
fn claude_auth_commands_use_provider_auth_namespace() {
    let login = login_command(
        &login_request(Engine::Claude, AuthMode::Choose, None),
        &profile(Engine::Claude, None),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        login.args,
        vec![OsString::from("auth"), OsString::from("login")]
    );

    let logout = logout_command(
        &request(Engine::Claude, AccountOperation::Logout, None),
        &profile(Engine::Claude, Some(DetectedAuth::OAuth)),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        &logout.args[..2],
        [OsString::from("auth"), OsString::from("logout")]
    );
}

#[test]
fn kimi_device_login_uses_its_oauth_device_flow() {
    let invocation = login_command(
        &login_request(Engine::Kimi, AuthMode::Device, Some("global")),
        &profile(Engine::Kimi, None),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        invocation.args,
        vec![
            OsString::from("login"),
            OsString::from("--region"),
            OsString::from("global"),
        ]
    );
}

#[test]
fn kimi_oauth_login_forwards_the_supported_region() {
    let invocation = login_command(
        &login_request(Engine::Kimi, AuthMode::OAuth, Some("mainland-cn")),
        &profile(Engine::Kimi, None),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        invocation.args,
        vec![
            OsString::from("login"),
            OsString::from("--region"),
            OsString::from("mainland-cn"),
        ]
    );
}

#[test]
fn opencode_run_sets_qualified_model_policy_and_profile_data_home() {
    let invocation = run_command(
        &request(
            Engine::Opencode,
            AccountOperation::Run,
            Some("local/big-pickle"),
        ),
        &profile(Engine::Opencode, Some(DetectedAuth::ApiKey)),
        "local/big-pickle",
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        invocation.environment.get(&OsString::from("XDG_DATA_HOME")),
        Some(&OsString::from("/fixture/profile"))
    );
    assert!(
        invocation
            .environment
            .contains_key(&OsString::from("OPENCODE_CONFIG_CONTENT"))
    );
    assert!(
        invocation
            .args
            .contains(&OsString::from("local/big-pickle"))
    );
}

#[test]
fn opencode_logout_uses_auth_namespace() {
    let invocation = logout_command(
        &request(Engine::Opencode, AccountOperation::Logout, None),
        &profile(Engine::Opencode, Some(DetectedAuth::ApiKey)),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert_eq!(
        invocation.args,
        vec![
            OsString::from("auth"),
            OsString::from("logout"),
            OsString::from("--prompt"),
        ]
    );
}

#[test]
fn opencode_login_modes_use_auth_provider_method_flags() {
    for (mode, expected_method) in [
        (AuthMode::Choose, None),
        (AuthMode::ApiKey, Some("api")),
        (AuthMode::OAuth, Some("oauth")),
    ] {
        let invocation = login_command(
            &AccountHelperRequest {
                engine: Engine::Opencode,
                account: super::super::request::AccountSelector::Number(2),
                operation: AccountOperation::Login {
                    auth_mode: mode,
                    provider: Some("openrouter".into()),
                    region: None,
                },
                model: None,
                forwarded: Vec::new(),
                json: false,
            },
            &profile(Engine::Opencode, None),
            Path::new("/fixture/home"),
            Path::new("/fixture/workspace"),
        )
        .unwrap();
        assert_eq!(
            &invocation.args[..4],
            [
                OsString::from("auth"),
                OsString::from("login"),
                OsString::from("--provider"),
                OsString::from("openrouter"),
            ]
        );
        match expected_method {
            Some(method) => assert_eq!(
                &invocation.args[4..],
                [OsString::from("--method"), OsString::from(method)]
            ),
            None => assert_eq!(invocation.args.len(), 4),
        }
    }
}

#[test]
fn opencode_whoami_uses_auth_listing() {
    let invocation = whoami_command(
        &request(Engine::Opencode, AccountOperation::Whoami, None),
        &profile(Engine::Opencode, Some(DetectedAuth::OAuth)),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .expect("OpenCode account listing");
    assert_eq!(
        invocation.args,
        [OsString::from("auth"), OsString::from("list")]
    );
}

#[test]
fn kimi_choose_starts_the_interactive_selector() {
    let invocation = login_command(
        &login_request(Engine::Kimi, AuthMode::Choose, None),
        &profile(Engine::Kimi, None),
        Path::new("/fixture/home"),
        Path::new("/fixture/workspace"),
    )
    .unwrap();
    assert!(invocation.interactive);
    assert!(invocation.args.is_empty());
}
