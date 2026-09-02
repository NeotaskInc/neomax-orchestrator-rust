use anyhow::{Result, bail};
use neomax_core::Engine;

use crate::models::validate_model;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccountSelector {
    Number(u32),
    Orchestrator,
}

impl AccountSelector {
    pub(crate) fn default_account() -> Self {
        Self::Number(1)
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Orchestrator => "orch".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthMode {
    Choose,
    OAuth,
    Device,
    ApiKey,
    AccessToken,
}

impl AuthMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "choose" | "select" => Some(Self::Choose),
            "oauth" | "oidc" => Some(Self::OAuth),
            "device" | "device-code" | "device_auth" => Some(Self::Device),
            "api-key" | "apikey" | "api_key" => Some(Self::ApiKey),
            "access-token" | "access_token" | "token" => Some(Self::AccessToken),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccountOperation {
    Login {
        auth_mode: AuthMode,
        provider: Option<String>,
        region: Option<String>,
    },
    Logout,
    Run,
    Models,
    Status,
    Whoami,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccountHelperRequest {
    pub(crate) engine: Engine,
    pub(crate) account: AccountSelector,
    pub(crate) operation: AccountOperation,
    pub(crate) model: Option<String>,
    pub(crate) forwarded: Vec<String>,
    pub(crate) json: bool,
}

impl AccountHelperRequest {
    pub(crate) fn parse(engine: Engine, args: &[String]) -> Result<Self> {
        let mut remaining = Vec::with_capacity(args.len());
        let mut model = None;
        let mut json = false;
        let model_flag = format!("--{}-model", engine);
        let mut index = 0;
        while index < args.len() {
            let current = &args[index];
            if current == "--json" {
                json = true;
                index += 1;
                continue;
            }
            if current == "--model" || current == "-m" || current == &model_flag {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("{current} requires a model value"))?;
                model = Some(validate_model(value.clone())?);
                index += 2;
                continue;
            }
            let mut consumed_model_flag = false;
            for flag in [
                "--model=",
                "-m=",
                "--claude-model=",
                "--codex-model=",
                "--opencode-model=",
                "--kimi-model=",
                "--grok-model=",
            ] {
                if let Some(value) = current.strip_prefix(flag) {
                    if value.is_empty() {
                        let name = flag.trim_end_matches('=');
                        bail!("{name} requires a model value");
                    }
                    model = Some(validate_model(value.to_owned())?);
                    consumed_model_flag = true;
                    break;
                }
            }
            if consumed_model_flag {
                index += 1;
                continue;
            }
            remaining.push(current.clone());
            index += 1;
        }

        let (operation, account, forwarded) = parse_operation(engine, &remaining)?;
        Ok(Self {
            engine,
            account,
            operation,
            model,
            forwarded,
            json,
        })
    }

    pub(crate) fn with_auth_mode(&self, auth_mode: AuthMode) -> Self {
        let mut request = self.clone();
        if let AccountOperation::Login {
            auth_mode: current, ..
        } = &mut request.operation
        {
            *current = auth_mode;
        }
        request
    }
}

fn parse_operation(
    engine: Engine,
    args: &[String],
) -> Result<(AccountOperation, AccountSelector, Vec<String>)> {
    let Some(first) = args.first() else {
        return Ok((
            AccountOperation::Status,
            AccountSelector::default_account(),
            Vec::new(),
        ));
    };
    if let Some(account) = parse_selector(first) {
        return Ok((
            AccountOperation::Login {
                auth_mode: AuthMode::Choose,
                provider: None,
                region: None,
            },
            account,
            args[1..].to_vec(),
        ));
    }

    let command = first.to_ascii_lowercase();
    match command.as_str() {
        "login" => parse_login(engine, args),
        "logout" => parse_simple(AccountOperation::Logout, args),
        "run" => parse_accounted(AccountOperation::Run, args),
        "models" => parse_accounted(AccountOperation::Models, args),
        "status" => parse_status(args),
        "whoami" => parse_accounted(AccountOperation::Whoami, args),
        "help" | "--help" | "-h" => bail!("account helper help is handled by the launcher"),
        _ => bail!("unknown account-helper operation {first}"),
    }
}

fn parse_login(
    engine: Engine,
    args: &[String],
) -> Result<(AccountOperation, AccountSelector, Vec<String>)> {
    let account = args
        .get(1)
        .and_then(|value| parse_selector(value))
        .ok_or_else(|| anyhow::anyhow!("login requires an account number or orch"))?;
    let mut index = 2;
    let mut auth_mode = AuthMode::Choose;
    let mut provider = None;
    let mut region = None;
    let mut forwarded = Vec::new();
    while let Some(value) = args.get(index) {
        if let Some(mode) = AuthMode::parse(value) {
            auth_mode = mode;
            index += 1;
            continue;
        }
        if engine == Engine::Opencode && provider.is_none() && !value.starts_with('-') {
            provider = Some(value.clone());
            index += 1;
            continue;
        }
        if engine == Engine::Kimi
            && region.is_none()
            && matches!(
                auth_mode,
                AuthMode::OAuth | AuthMode::Device | AuthMode::Choose
            )
            && matches!(value.as_str(), "global" | "mainland-cn")
        {
            region = Some(value.clone());
            index += 1;
            continue;
        }
        if value == "--provider" {
            provider = Some(
                args.get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("--provider requires a value"))?
                    .clone(),
            );
            index += 2;
            continue;
        }
        if let Some(value) = value.strip_prefix("--provider=") {
            if value.is_empty() {
                bail!("--provider requires a value");
            }
            provider = Some(value.into());
            index += 1;
            continue;
        }
        if value == "--region" {
            region = Some(
                args.get(index + 1)
                    .ok_or_else(|| anyhow::anyhow!("--region requires a value"))?
                    .clone(),
            );
            index += 2;
            continue;
        }
        if let Some(value) = value.strip_prefix("--region=") {
            if value.is_empty() {
                bail!("--region requires a value");
            }
            region = Some(value.into());
            index += 1;
            continue;
        }
        forwarded.push(value.clone());
        index += 1;
    }
    if auth_mode == AuthMode::AccessToken && !forwarded.is_empty() {
        bail!(
            "Codex access-token login reads the token from stdin and accepts no additional arguments"
        );
    }
    validate_login_options(engine, auth_mode, region.as_deref())?;
    Ok((
        AccountOperation::Login {
            auth_mode,
            provider,
            region,
        },
        account,
        forwarded,
    ))
}

fn parse_simple(
    operation: AccountOperation,
    args: &[String],
) -> Result<(AccountOperation, AccountSelector, Vec<String>)> {
    let account = args
        .get(1)
        .and_then(|value| parse_selector(value))
        .ok_or_else(|| anyhow::anyhow!("operation requires an account number or orch"))?;
    Ok((operation, account, args[2..].to_vec()))
}

fn parse_accounted(
    operation: AccountOperation,
    args: &[String],
) -> Result<(AccountOperation, AccountSelector, Vec<String>)> {
    let account = args
        .get(1)
        .and_then(|value| parse_selector(value))
        .unwrap_or_else(AccountSelector::default_account);
    let start = args
        .get(1)
        .and_then(|value| parse_selector(value))
        .map_or(1, |_| 2);
    Ok((operation, account, args[start..].to_vec()))
}

fn parse_status(args: &[String]) -> Result<(AccountOperation, AccountSelector, Vec<String>)> {
    if args.len() > 1 {
        bail!("status does not accept an account or positional argument");
    }
    Ok((
        AccountOperation::Status,
        AccountSelector::default_account(),
        Vec::new(),
    ))
}

fn parse_selector(value: &str) -> Option<AccountSelector> {
    if value.eq_ignore_ascii_case("orch") || value.eq_ignore_ascii_case("orchestrator") {
        return Some(AccountSelector::Orchestrator);
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|number| *number > 0)
        .map(AccountSelector::Number)
}

fn validate_login_options(engine: Engine, mode: AuthMode, region: Option<&str>) -> Result<()> {
    if let Some(region) = region {
        if engine != Engine::Kimi {
            bail!("--region is supported only by Kimi OAuth/device login");
        }
        if !matches!(region, "global" | "mainland-cn") {
            bail!("Kimi OAuth region must be global or mainland-cn");
        }
    }
    if engine == Engine::Kimi
        && region.is_some()
        && !matches!(mode, AuthMode::OAuth | AuthMode::Device | AuthMode::Choose)
    {
        bail!("Kimi --region requires OAuth or device login");
    }
    if engine == Engine::Opencode && mode == AuthMode::Device {
        bail!("OpenCode login supports API-key or OAuth provider login, not device login");
    }
    if mode == AuthMode::AccessToken && engine != Engine::Codex {
        bail!("access-token login is supported only by Codex");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
