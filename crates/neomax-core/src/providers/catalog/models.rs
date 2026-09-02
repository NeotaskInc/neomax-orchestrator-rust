use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Engine, Error, Result};

use super::environment::Environment;
use super::specs::{default_model_id, spec};
use super::types::{ModelOrigin, ResolvedModel};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDefaults {
    pub claude: String,
    pub codex: String,
    pub opencode: String,
    pub kimi: String,
    pub grok: String,
}

impl Default for ModelDefaults {
    fn default() -> Self {
        Self::from_catalog()
    }
}

impl ModelDefaults {
    pub fn from_catalog() -> Self {
        Self {
            claude: default_model_id(Engine::Claude).into(),
            codex: default_model_id(Engine::Codex).into(),
            opencode: default_model_id(Engine::Opencode).into(),
            kimi: default_model_id(Engine::Kimi).into(),
            grok: default_model_id(Engine::Grok).into(),
        }
    }

    pub fn for_engine(&self, engine: Engine) -> &str {
        match engine {
            Engine::Claude => &self.claude,
            Engine::Codex => &self.codex,
            Engine::Opencode => &self.opencode,
            Engine::Kimi => &self.kimi,
            Engine::Grok => &self.grok,
        }
    }
}

pub fn resolve_model(
    engine: Engine,
    explicit: Option<&str>,
    environment: &dyn Environment,
) -> Result<ResolvedModel> {
    if let Some(value) = explicit {
        return Ok(ResolvedModel {
            id: canonical_alias(engine, validate(value, engine)?)?,
            origin: ModelOrigin::Explicit,
        });
    }
    let provider = spec(engine);
    if let Some(value) = environment.value(&provider.model_env) {
        return Ok(ResolvedModel {
            id: canonical_alias(engine, validate(&value, engine)?)?,
            origin: ModelOrigin::ProviderEnvironment,
        });
    }
    if engine == Engine::Claude {
        if let Some(value) = environment.value("NEOMAX_DEFAULT_MODEL") {
            let value = validate(&value, engine)?;
            let value = if value.ends_with("[1m]") {
                value.to_string()
            } else {
                format!("{value}[1m]")
            };
            return Ok(ResolvedModel {
                id: value,
                origin: ModelOrigin::GlobalEnvironment,
            });
        }
    }
    Ok(ResolvedModel {
        id: provider.default_model,
        origin: ModelOrigin::StrictDefault,
    })
}

pub fn default_models() -> BTreeMap<Engine, ResolvedModel> {
    Engine::ALL
        .into_iter()
        .map(|engine| {
            (
                engine,
                ResolvedModel {
                    id: spec(engine).default_model,
                    origin: ModelOrigin::StrictDefault,
                },
            )
        })
        .collect()
}

fn validate(value: &str, engine: Engine) -> Result<&str> {
    let value = value.trim();
    if value.is_empty()
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(Error::InvalidArgument(format!(
            "{} model requires a non-empty model ID without whitespace",
            engine.as_str()
        )));
    }
    if engine == Engine::Opencode
        && !value
            .split_once('/')
            .is_some_and(|(provider, model)| !provider.is_empty() && !model.is_empty())
    {
        return Err(Error::InvalidArgument(format!(
            "OpenCode model must use provider/model form: {value}"
        )));
    }
    Ok(value)
}

fn canonical_alias(engine: Engine, value: &str) -> Result<String> {
    let lower = value.to_ascii_lowercase();
    let canonical = match engine {
        Engine::Codex => match lower.as_str() {
            "sol" | "gpt-5.6" | "gpt5.6" => "gpt-5.6-sol",
            "terra" => "gpt-5.6-terra",
            "luna" => "gpt-5.6-luna",
            _ => value,
        },
        Engine::Kimi => match lower.as_str() {
            "k3" | "kimi-k3" => "kimi-code/k3",
            "k2.7" | "k27" | "2.7" => "kimi-code/kimi-for-coding",
            _ => value,
        },
        _ => value,
    };
    Ok(canonical.into())
}
