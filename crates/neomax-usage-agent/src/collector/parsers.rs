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
    if !matches!(
        value.get("type").and_then(serde_json::Value::as_str),
        Some("turn_context" | "session_meta")
    ) {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get("model"))
        .and_then(serde_json::Value::as_str)
        .filter(|model| !model.trim().is_empty())
        .map(str::to_owned)
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
