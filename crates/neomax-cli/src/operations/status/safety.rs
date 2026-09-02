pub(super) fn account_label(value: &str) -> String {
    let value = safe_component(value);
    if value.is_empty() {
        return "unknown".into();
    }
    let result: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(96)
        .collect();
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

pub(super) fn session_label(value: &str) -> String {
    let result: String = safe_component(value)
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(160)
        .collect();
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

pub(super) fn text_label(value: &str) -> String {
    let result: String = safe_component(value)
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(160)
        .collect();
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

pub(super) fn tag_label(value: &str) -> String {
    let result: String = value
        .chars()
        .filter(|character| !character.is_control() && !matches!(character, '/' | '\\'))
        .take(120)
        .collect();
    if result.trim().is_empty() {
        "unknown".into()
    } else {
        result
    }
}

pub(super) fn model_label(value: &str) -> String {
    let value = value.trim();
    let value = if value.starts_with('/') || value.contains('\\') {
        safe_component(value)
    } else {
        value
    };
    let result: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '.' | '_' | '-' | '/' | ':' | '[' | ']' | '(' | ')'
                )
        })
        .take(160)
        .collect();
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

fn safe_component(value: &str) -> &str {
    value.trim().rsplit(['/', '\\']).next().unwrap_or_default()
}

pub(super) fn program_label(value: &str) -> String {
    std::path::Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .map(account_label)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unavailable".into())
}
