use std::collections::BTreeSet;

use serde_json::Value;

use crate::providers::catalog::CLAUDE_OPUS_MODEL;
use crate::settings::resolve_explicit_model;
use crate::{Engine, Error, Result, WorkerScope};

use super::types::Part;

pub const PROVIDER_ORDER: [Engine; 5] = [
    Engine::Claude,
    Engine::Opencode,
    Engine::Grok,
    Engine::Kimi,
    Engine::Codex,
];

pub(crate) fn normalize_part(value: &Value, index: usize, scope: &WorkerScope) -> Result<Part> {
    let object = value.as_object().ok_or_else(|| {
        Error::InvalidArgument(format!(
            "neomax run-all: part #{} is not an object",
            index + 1
        ))
    })?;

    let id = match object.get("id") {
        None | Some(Value::Null) => format!("p{}", index + 1),
        Some(Value::String(value)) if value.trim().is_empty() => format!("p{}", index + 1),
        Some(value) => value_string(value).unwrap_or_else(|| value.to_string()),
    };
    validate_part_id(&id)?;

    let prompt = object
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| Error::InvalidArgument(format!("neomax run-all: part {id} has no prompt")))?
        .to_string();

    let engine = match object.get("engine") {
        None => default_engine(scope),
        Some(Value::String(raw)) => raw.parse::<Engine>().map_err(|_| {
            Error::InvalidArgument(format!(
                "neomax run-all: part {id} has unknown engine {raw:?}"
            ))
        })?,
        Some(other) => {
            return Err(Error::InvalidArgument(format!(
                "neomax run-all: part {id} has unknown engine {other}"
            )));
        }
    };
    if !scope.contains(engine) {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} uses engine {engine:?}, out of fleet scope ({})",
            scope.csv()
        )));
    }

    let effort = optional_string_field(object.get("effort"), &id, "effort")?;
    validate_effort(&id, engine, effort.as_deref())?;

    let ultra = bool_field(object.get("ultra"), &id, "ultra")?;
    if ultra && matches!(engine, Engine::Opencode | Engine::Kimi | Engine::Grok) {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} sets ultra; {engine} does not support it"
        )));
    }

    let opus = bool_field(object.get("opus"), &id, "opus")?;
    if opus && engine != Engine::Claude {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} sets opus, which is a Claude-only option"
        )));
    }

    let explicit_model = optional_string_field(object.get("model"), &id, "model")?;
    let codex_model = optional_string_field(object.get("codex_model"), &id, "codex_model")?;
    let kimi_model = optional_string_field(object.get("kimi_model"), &id, "kimi_model")?;
    if codex_model.is_some() && engine != Engine::Codex {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} sets codex_model on a {engine} part"
        )));
    }
    if kimi_model.is_some() && engine != Engine::Kimi {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} sets kimi_model on a {engine} part"
        )));
    }
    if explicit_model.is_some() && (codex_model.is_some() || kimi_model.is_some()) {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} cannot combine model with a legacy engine model key"
        )));
    }

    let model = explicit_model
        .as_deref()
        .map(|value| resolve_explicit_model(engine, value))
        .transpose()?;
    let codex_model = codex_model
        .as_deref()
        .map(|value| resolve_explicit_model(Engine::Codex, value))
        .transpose()?;
    let kimi_model = kimi_model
        .as_deref()
        .map(|value| resolve_explicit_model(Engine::Kimi, value))
        .transpose()?;

    if opus {
        if let Some(model) = model.as_deref() {
            let normalized = model.strip_suffix("[1m]").unwrap_or(model);
            if normalized != CLAUDE_OPUS_MODEL {
                return Err(Error::InvalidArgument(format!(
                    "neomax run-all: part {id} combines opus with a different Claude model"
                )));
            }
        }
    }

    let area = normalize_keys(object.get("area"));
    let depends_on = normalize_keys(object.get("depends_on"));

    Ok(Part {
        id,
        prompt,
        engine,
        model,
        area,
        depends_on,
        effort,
        ultra,
        opus,
        codex_model,
        kimi_model,
        order: index,
        extra: object
            .iter()
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "id" | "prompt"
                        | "engine"
                        | "model"
                        | "area"
                        | "depends_on"
                        | "effort"
                        | "ultra"
                        | "opus"
                        | "codex_model"
                        | "kimi_model"
                        | "order"
                )
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    })
}

pub(crate) fn normalize_keys(value: Option<&Value>) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    let values = match value {
        Value::String(value) => vec![Value::String(value.clone())],
        Value::Array(values) => values.clone(),
        _ => Vec::new(),
    };
    values
        .into_iter()
        .map(|value| value_to_key(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn default_engine(scope: &WorkerScope) -> Engine {
    PROVIDER_ORDER
        .into_iter()
        .find(|engine| scope.contains(*engine))
        .unwrap_or(Engine::Claude)
}

pub fn validate_part_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Error::InvalidArgument(format!(
            "neomax run-all: invalid part id {id:?} (use [A-Za-z0-9._-])"
        )));
    }
    Ok(())
}

fn validate_effort(id: &str, engine: Engine, effort: Option<&str>) -> Result<()> {
    let Some(effort) = effort else {
        return Ok(());
    };
    let allowed: &[&str] = match engine {
        Engine::Codex => &["low", "medium", "high", "xhigh"],
        Engine::Claude => &["low", "medium", "high", "xhigh", "max"],
        Engine::Opencode | Engine::Kimi | Engine::Grok => &[],
    };
    if allowed.contains(&effort) {
        return Ok(());
    }
    Err(Error::InvalidArgument(format!(
        "neomax run-all: part {id} effort {effort:?} invalid for {engine} ({})",
        allowed.join("|")
    )))
}

fn bool_field(value: Option<&Value>, id: &str, field: &str) -> Result<bool> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(other) => Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} field {field} must be boolean, got {other}"
        ))),
    }
}

fn optional_string_field(value: Option<&Value>, id: &str, field: &str) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok((!value.trim().is_empty()).then(|| value.trim().into())),
        Some(other) => Err(Error::InvalidArgument(format!(
            "neomax run-all: part {id} field {field} must be a string, got {other}"
        ))),
    }
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_key(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}
