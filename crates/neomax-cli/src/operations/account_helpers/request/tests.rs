use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn numeric_and_orchestrator_shorthand_are_login() {
    let numeric = AccountHelperRequest::parse(Engine::Codex, &args(&["2"])).unwrap();
    assert_eq!(numeric.account, AccountSelector::Number(2));
    assert!(matches!(numeric.operation, AccountOperation::Login { .. }));

    let orch = AccountHelperRequest::parse(Engine::Grok, &args(&["orchestrator"])).unwrap();
    assert_eq!(orch.account, AccountSelector::Orchestrator);
    assert!(matches!(orch.operation, AccountOperation::Login { .. }));
}

#[test]
fn grok_choose_is_preserved_until_the_auth_port_resolves_it() {
    let request =
        AccountHelperRequest::parse(Engine::Grok, &args(&["login", "2", "choose"])).unwrap();
    assert!(matches!(
        request.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::Choose,
            ..
        }
    ));
}

#[test]
fn provider_specific_login_options_are_preserved_and_validated() {
    let kimi = AccountHelperRequest::parse(
        Engine::Kimi,
        &args(&["login", "2", "oauth", "--region", "global"]),
    )
    .unwrap();
    assert!(matches!(
        kimi.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::OAuth,
            region: Some(ref region),
            ..
        } if region == "global"
    ));
    let grok =
        AccountHelperRequest::parse(Engine::Grok, &args(&["login", "orch", "device"])).unwrap();
    assert!(matches!(
        grok.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::Device,
            ..
        }
    ));
    let kimi_device =
        AccountHelperRequest::parse(Engine::Kimi, &args(&["login", "2", "device", "global"]))
            .unwrap();
    assert!(matches!(
        kimi_device.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::Device,
            region: Some(ref region),
            ..
        } if region == "global"
    ));
    assert!(
        AccountHelperRequest::parse(
            Engine::Kimi,
            &args(&["login", "1", "api-key", "--region", "global"])
        )
        .is_err()
    );
    let opencode =
        AccountHelperRequest::parse(Engine::Opencode, &args(&["login", "2", "openrouter"]))
            .unwrap();
    assert!(matches!(
        opencode.operation,
        AccountOperation::Login {
            provider: Some(ref provider),
            ..
        } if provider == "openrouter"
    ));
    let opencode_oauth = AccountHelperRequest::parse(
        Engine::Opencode,
        &args(&["login", "2", "openrouter", "oauth"]),
    )
    .unwrap();
    assert!(matches!(
        opencode_oauth.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::OAuth,
            provider: Some(ref provider),
            ..
        } if provider == "openrouter"
    ));
    let opencode_api =
        AccountHelperRequest::parse(Engine::Opencode, &args(&["login", "2", "api-key"])).unwrap();
    assert!(matches!(
        opencode_api.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::ApiKey,
            ..
        }
    ));
    let opencode_device =
        AccountHelperRequest::parse(Engine::Opencode, &args(&["login", "2", "device"]));
    assert!(opencode_device.is_err());

    let codex_access_token =
        AccountHelperRequest::parse(Engine::Codex, &args(&["login", "2", "access-token"])).unwrap();
    assert!(matches!(
        codex_access_token.operation,
        AccountOperation::Login {
            auth_mode: AuthMode::AccessToken,
            ..
        }
    ));
    assert!(
        AccountHelperRequest::parse(Engine::Claude, &args(&["login", "2", "access-token"]))
            .is_err()
    );
    assert!(
        AccountHelperRequest::parse(
            Engine::Codex,
            &args(&["login", "2", "access-token", "raw-token"])
        )
        .is_err()
    );
}

#[test]
fn models_and_run_accept_optional_accounts_and_forward_arguments() {
    let run = AccountHelperRequest::parse(
        Engine::Opencode,
        &args(&["run", "3", "--model", "local/big-pickle", "--auto"]),
    )
    .unwrap();
    assert_eq!(run.account, AccountSelector::Number(3));
    assert_eq!(run.model.as_deref(), Some("local/big-pickle"));
    assert_eq!(run.forwarded, args(&["--auto"]));

    let models = AccountHelperRequest::parse(Engine::Kimi, &args(&["models"])).unwrap();
    assert_eq!(models.account, AccountSelector::Number(1));
    assert!(matches!(models.operation, AccountOperation::Models));
}
