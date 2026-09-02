use neomax_core::config::Engine;
use sha2::{Digest, Sha256};

pub(crate) fn validate_numeric_usage(line: &str, engine: Engine) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return true;
    };
    let paths: &[&[&str]] = match engine {
        Engine::Claude => &[
            &["message", "usage", "input_tokens"],
            &["message", "usage", "output_tokens"],
            &["message", "usage", "cache_creation_input_tokens"],
            &["message", "usage", "cache_read_input_tokens"],
        ],
        Engine::Codex => &[
            &["payload", "info", "total_token_usage", "input_tokens"],
            &[
                "payload",
                "info",
                "total_token_usage",
                "cached_input_tokens",
            ],
            &["payload", "info", "total_token_usage", "output_tokens"],
        ],
        Engine::Kimi => &[
            &["usage", "inputOther"],
            &["usage", "output"],
            &["usage", "inputCacheCreation"],
            &["usage", "inputCacheRead"],
        ],
        _ => &[],
    };
    paths.iter().all(|path| {
        let mut current = &value;
        for part in *path {
            let Some(next) = current.get(*part) else {
                return true;
            };
            current = next;
        }
        current.is_number() || current.is_null()
    })
}

pub(crate) fn codex_model_in_line(line: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    find_string(&value, "model").filter(|model| model.starts_with("gpt-"))
}

fn find_string(value: &serde_json::Value, key: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(value) = map.get(key).and_then(|value| value.as_str()) {
                return Some(value.to_owned());
            }
            map.values().find_map(|value| find_string(value, key))
        }
        serde_json::Value::Array(values) => values.iter().find_map(|value| find_string(value, key)),
        _ => None,
    }
}

pub(crate) fn stable_digest(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    let bytes = digest.finalize();
    bytes
        .iter()
        .take(10)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
