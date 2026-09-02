use std::path::Path;

use serde_json::Value;

pub fn timestamp_epoch(value: &Value) -> Option<i64> {
    if let Some(number) = value.as_i64() {
        return Some(normalize_epoch(number));
    }
    if let Some(number) = value.as_f64() {
        return Some(normalize_epoch(number as i64));
    }
    let text = value.as_str()?.trim();
    if let Ok(number) = text.parse::<i64>() {
        return Some(normalize_epoch(number));
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|stamp| stamp.timestamp())
}

pub fn session_id_from_path(path: &Path, engine: crate::Engine) -> String {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim_end_matches(".jsonl");
    match engine {
        crate::Engine::Codex => {
            let value = name.strip_prefix("rollout-").unwrap_or(name);
            if value.len() > 36 {
                value[value.len() - 36..].to_string()
            } else {
                value.to_string()
            }
        }
        _ => name.to_string(),
    }
}

pub fn workflow_id(path: &Path) -> Option<String> {
    let components = path.components().collect::<Vec<_>>();
    let index = components
        .iter()
        .position(|component| component.as_os_str() == "workflows")?;
    components
        .get(index + 1)
        .and_then(|component| component.as_os_str().to_str())
        .filter(|value| value.starts_with("wf_"))
        .map(str::to_string)
}

fn normalize_epoch(mut value: i64) -> i64 {
    while value.unsigned_abs() > 100_000_000_000 {
        value /= 1000;
    }
    value
}
