use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::atomic::write_bytes_atomic;
use neomax_core::io::{set_private_directory, set_private_path};
use neomax_core::providers::catalog::{FileSystem, RealFileSystem};

use super::super::prompt::PromptPort;
use super::super::request::AuthMode;

pub(super) const GROK_API_KEY_ENVIRONMENT: &[&str] = &[
    "NEOMAX_GROK_API_KEY",
    "XAI_API_KEY",
    "GROK_API_KEY",
    "GROK_DEPLOYMENT_KEY",
];

pub(super) fn api_key_from_environment(engine: Engine) -> Result<String> {
    api_key_from_values(engine, |variable| std::env::var(variable).ok())
}

pub(super) fn api_key_from_values<F>(engine: Engine, mut value_for: F) -> Result<String>
where
    F: FnMut(&str) -> Option<String>,
{
    let variables = match engine {
        Engine::Kimi => ["NEOMAX_KIMI_API_KEY", "KIMI_API_KEY"].as_slice(),
        Engine::Grok => GROK_API_KEY_ENVIRONMENT,
        _ => bail!("{engine} does not support API-key login"),
    };
    for variable in variables {
        if let Some(value) = value_for(variable) {
            if !value.trim().is_empty() {
                return Ok(value.trim().to_owned());
            }
        }
    }
    bail!(
        "set {} before {} API-key login",
        variables.join(" or "),
        engine
    )
}

pub(super) fn api_key(engine: Engine, prompt: &dyn PromptPort) -> Result<String> {
    match api_key_from_environment(engine) {
        Ok(key) => Ok(key),
        Err(_error) if engine == Engine::Grok => {
            let key = prompt.secret("xAI API key: ")?;
            if key.trim().is_empty() {
                bail!("Grok API key must not be empty")
            }
            Ok(key.trim().to_owned())
        }
        Err(error) => Err(error),
    }
}

pub(super) fn choose_auth_mode(engine: Engine, prompt: &dyn PromptPort) -> Result<AuthMode> {
    if engine != Engine::Grok {
        bail!("{engine} does not expose a multi-method account selector")
    }
    let choice = prompt.selection(
        "Choose Grok authentication: 1) browser OAuth  2) device OAuth  3) API key\nSelection [1]: ",
    )?;
    match choice.trim().to_ascii_lowercase().as_str() {
        "" | "1" | "oauth" | "browser" | "browser-oauth" => Ok(AuthMode::OAuth),
        "2" | "device" | "device-code" | "device-oauth" => Ok(AuthMode::Device),
        "3" | "api-key" | "apikey" | "api_key" | "key" => Ok(AuthMode::ApiKey),
        _ => bail!("Grok authentication selection must be 1, 2, or 3"),
    }
}

pub(super) fn set_preferred_auth(engine: Engine, profile: &Path, mode: AuthMode) -> Result<()> {
    if engine != Engine::Grok {
        return Ok(());
    }
    let method = match mode {
        AuthMode::OAuth | AuthMode::Device => "oidc",
        AuthMode::ApiKey => "api_key",
        AuthMode::Choose => bail!("Grok authentication selection must be resolved first"),
        AuthMode::AccessToken => {
            bail!("Grok access-token login is not supported; use OAuth, device, or API key")
        }
    };
    let config_path = profile.join("config.toml");
    prepare_private_profile(profile, &[&config_path])?;
    let existing = match RealFileSystem.read(&config_path)? {
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|_| anyhow::anyhow!("Grok config.toml is not valid UTF-8"))?,
        None => String::new(),
    };
    let config = preferred_auth_config(&existing, method);
    write_bytes_atomic(&config_path, config.as_bytes()).map_err(anyhow::Error::from)?;
    set_private_path(&config_path)?;
    Ok(())
}

pub(super) fn configure(engine: Engine, profile: &Path, secret: &str) -> Result<()> {
    if secret.trim().is_empty() {
        bail!("{engine} API key must not be empty");
    }
    if engine != Engine::Grok && engine != Engine::Kimi {
        bail!("API-key profile configuration is unavailable for {engine}");
    }
    match engine {
        Engine::Grok => configure_grok_api_key(profile, secret),
        Engine::Kimi => configure_kimi_api_key(profile, secret),
        _ => unreachable!(),
    }
}

fn configure_grok_api_key(profile: &Path, key: &str) -> Result<()> {
    let auth_path = profile.join("auth.json");
    let config_path = profile.join("config.toml");
    prepare_private_profile(profile, &[&auth_path, &config_path])?;
    let mut store = match RealFileSystem.read(&auth_path)? {
        Some(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default(),
        None => serde_json::Map::new(),
    };
    store.insert(
        "xai::api_key".into(),
        serde_json::json!({
            "key": key,
            "auth_mode": "api_key",
            "email": null,
            "user_id": "",
            "team_blocked_reasons": [],
            "coding_data_retention_opt_out": true
        }),
    );
    let bytes = serde_json::to_vec_pretty(&serde_json::Value::Object(store))?;
    write_bytes_atomic(&auth_path, &bytes).map_err(anyhow::Error::from)?;
    set_private_path(&auth_path)?;
    let config = match RealFileSystem.read(&config_path)? {
        Some(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        None => String::new(),
    };
    let config = preferred_auth_config(&config, "api_key");
    write_bytes_atomic(&config_path, config.as_bytes()).map_err(anyhow::Error::from)?;
    set_private_path(&config_path)?;
    Ok(())
}

fn configure_kimi_api_key(profile: &Path, key: &str) -> Result<()> {
    let config_path = profile.join("config.toml");
    prepare_private_profile(profile, &[&config_path])?;
    let existing = match RealFileSystem.read(&config_path)? {
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|_| anyhow::anyhow!("Kimi config.toml is not valid UTF-8"))?,
        None => String::new(),
    };
    let config = update_kimi_config(&existing, key)?;
    write_bytes_atomic(&config_path, config.as_bytes()).map_err(anyhow::Error::from)?;
    set_private_path(&config_path)?;
    Ok(())
}

fn prepare_private_profile(profile: &Path, files: &[&Path]) -> Result<()> {
    for path in files {
        match fs::symlink_metadata(path) {
            Ok(_) => set_private_path(path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    set_private_directory(profile)?;
    Ok(())
}

fn update_kimi_config(existing: &str, key: &str) -> Result<String> {
    let mut lines = existing.lines().map(str::to_owned).collect::<Vec<_>>();
    if lines.is_empty() {
        lines = vec!["default_model = \"kimi-code/k3\"".into()];
    } else {
        set_top_level(&mut lines, "default_model", "\"kimi-code/k3\"");
    }

    set_section_value(
        &mut lines,
        "[providers.\"managed:kimi-code\"]",
        "type",
        "\"kimi\"",
    );
    set_section_value(
        &mut lines,
        "[providers.\"managed:kimi-code\"]",
        "base_url",
        "\"https://api.kimi.com/coding/v1\"",
    );
    set_section_value(
        &mut lines,
        "[providers.\"managed:kimi-code\"]",
        "api_key",
        &format!("\"{}\"", toml_quote(key)),
    );
    set_kimi_model(&mut lines, "kimi-code/k3", "k3", 1_048_576, true);
    set_kimi_model(
        &mut lines,
        "kimi-code/kimi-for-coding",
        "kimi-for-coding",
        262_144,
        false,
    );
    let mut output = lines.join("\n");
    output.push('\n');
    Ok(output)
}

fn set_top_level(lines: &mut Vec<String>, key: &str, value: &str) {
    if let Some(index) = lines
        .iter()
        .position(|line| assignment_name(line) == Some(key))
    {
        lines[index] = format!("{key} = {value}");
        return;
    }
    let index = lines
        .iter()
        .position(|line| is_section_header(line))
        .unwrap_or(lines.len());
    lines.insert(index, format!("{key} = {value}"));
}

fn set_section_value(lines: &mut Vec<String>, section: &str, key: &str, value: &str) {
    let (start, end) = if let Some(bounds) = section_bounds(lines, section) {
        bounds
    } else {
        let nested_prefix = section.strip_suffix(']').unwrap_or(section).to_owned() + ".";
        let start = lines
            .iter()
            .position(|line| {
                let value = line.trim();
                value.starts_with('[')
                    && value.ends_with(']')
                    && value.starts_with(&format!("[{nested_prefix}"))
            })
            .unwrap_or(lines.len());
        if start == lines.len()
            && !lines.is_empty()
            && lines.last().is_some_and(|line| !line.trim().is_empty())
        {
            lines.push(String::new());
        }
        let start = if start == lines.len() {
            lines.push(section.into());
            lines.len() - 1
        } else {
            lines.insert(start, section.into());
            start
        };
        (start, start + 1)
    };
    if let Some(index) = (start + 1..end).find(|index| assignment_name(&lines[*index]) == Some(key))
    {
        lines[index] = format!("{key} = {value}");
    } else {
        lines.insert(end, format!("{key} = {value}"));
    }
}

fn set_kimi_model(
    lines: &mut Vec<String>,
    alias: &str,
    model: &str,
    context_size: u64,
    supports_effort: bool,
) {
    let section = format!("[models.\"{alias}\"]");
    set_section_value(lines, &section, "provider", "\"managed:kimi-code\"");
    set_section_value(lines, &section, "model", &format!("\"{model}\""));
    set_section_value(
        lines,
        &section,
        "max_context_size",
        &context_size.to_string(),
    );
    set_section_value(
        lines,
        &section,
        "capabilities",
        "[\"thinking\", \"always_thinking\", \"image_in\", \"video_in\", \"tool_use\"]",
    );
    if supports_effort {
        set_section_value(
            lines,
            &section,
            "support_efforts",
            "[\"low\", \"high\", \"max\"]",
        );
        set_section_value(lines, &section, "default_effort", "\"high\"");
    }
}

fn section_bounds(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| line.trim() == section)?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| is_section_header(line))
        .map_or(lines.len(), |(index, _)| index);
    Some((start, end))
}

fn is_section_header(line: &str) -> bool {
    let value = line.trim();
    value.starts_with('[') && value.ends_with(']')
}

fn assignment_name(line: &str) -> Option<&str> {
    let value = line.trim_start();
    if value.starts_with('#') {
        return None;
    }
    let (name, _) = value.split_once('=')?;
    let name = name.trim();
    (!name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        }))
    .then_some(name)
}

fn toml_quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn preferred_auth_config(existing: &str, method: &str) -> String {
    let mut output = Vec::new();
    let mut in_auth = false;
    let mut wrote = false;
    for line in existing.lines() {
        if let Some(section) = line
            .trim()
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            if in_auth && !wrote {
                output.push(format!("preferred_method = \"{method}\""));
                wrote = true;
            }
            in_auth = section.trim() == "auth";
        }
        if in_auth && line.trim_start().starts_with("preferred_method") {
            if !wrote {
                output.push(format!("preferred_method = \"{method}\""));
                wrote = true;
            }
            continue;
        }
        output.push(line.to_owned());
    }
    if in_auth && !wrote {
        output.push(format!("preferred_method = \"{method}\""));
        wrote = true;
    }
    if !wrote {
        if !output.is_empty() && output.last().is_some_and(|line| !line.trim().is_empty()) {
            output.push(String::new());
        }
        output.push("[auth]".into());
        output.push(format!("preferred_method = \"{method}\""));
    }
    let mut result = output.join("\n");
    result.push('\n');
    result
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
