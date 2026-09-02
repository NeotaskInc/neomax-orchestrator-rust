use crate::Result;

pub(super) fn reject_embedded(config: &str) -> Result<()> {
    let value = toml::from_str::<toml::Value>(config).map_err(|error| {
        crate::Error::InvalidArgument(format!(
            "Kimi plan mode requires a valid config.toml before staging a temporary home: {error}"
        ))
    })?;
    if let Some(field) = find_embedded(&value, None) {
        return Err(crate::Error::InvalidArgument(format!(
            "Kimi plan mode will not copy an inline credential from config.toml ({field}); use the OAuth profile credential directory or a provider configuration that keeps the secret outside the staged config"
        )));
    }
    Ok(())
}

fn find_embedded(value: &toml::Value, parent: Option<&str>) -> Option<String> {
    match value {
        toml::Value::Table(table) => table.iter().find_map(|(key, child)| {
            let current = parent
                .map(|prefix| format!("{prefix}.{key}"))
                .unwrap_or_else(|| key.clone());
            if is_inline_field(key, parent) && has_secret_value(child) {
                return Some(current);
            }
            find_embedded(child, Some(&current))
        }),
        toml::Value::Array(values) => values.iter().find_map(|child| find_embedded(child, parent)),
        _ => None,
    }
}

fn is_inline_field(key: &str, parent: Option<&str>) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
    if normalized == "apikey"
        || normalized == "accesstoken"
        || normalized == "refreshtoken"
        || normalized == "authtoken"
        || normalized == "clientsecret"
        || normalized == "secret"
        || normalized == "password"
    {
        return true;
    }
    normalized.ends_with("apikey")
        || normalized.ends_with("accesstoken")
        || normalized.ends_with("refreshtoken")
        || normalized.ends_with("authtoken")
        || (normalized == "key"
            && parent.is_some_and(|path| path.ends_with(".credentials") || path.ends_with(".auth")))
}

fn has_secret_value(value: &toml::Value) -> bool {
    match value {
        toml::Value::String(value) => !value.trim().is_empty(),
        toml::Value::Array(values) => values.iter().any(has_secret_value),
        toml::Value::Table(table) => table.values().any(has_secret_value),
        _ => false,
    }
}
