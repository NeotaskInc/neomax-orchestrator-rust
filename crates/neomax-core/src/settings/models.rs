use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_bytes_atomic;
use crate::io::{read_file, BoundedIoError, LocalFileSource, ReadLimits};
use crate::providers::catalog::{self, MapEnvironment, ModelOrigin};
use crate::{Engine, Error, Result};

const MAX_MODEL_SETTINGS_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelOverrides {
    pub claude: Option<String>,
    pub codex: Option<String>,
    pub opencode: Option<String>,
    pub kimi: Option<String>,
    pub grok: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveModel {
    pub engine: String,
    pub model: String,
    pub default: String,
    pub source: &'static str,
}

impl ModelOverrides {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match read_file(
            &LocalFileSource,
            path,
            ReadLimits::new(MAX_MODEL_SETTINGS_BYTES, std::time::Duration::from_secs(5))
                .expect("model settings read limits are valid"),
        ) {
            Ok(bytes) => bytes,
            Err(BoundedIoError::NotFound { .. }) => return Ok(Self::default()),
            Err(error) => return Err(error.into()),
        };
        let contents = String::from_utf8(bytes).map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let settings = toml::from_str::<Self>(&contents).map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        settings.validate().map_err(|error| Error::InvalidState {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let data = toml::to_string_pretty(self)
            .map_err(|error| Error::Message(format!("could not encode model settings: {error}")))?;
        write_bytes_atomic(path, data.as_bytes())
    }

    fn validate(&self) -> Result<()> {
        for engine in Engine::ALL {
            if let Some(model) = self.get(engine) {
                resolve_explicit_model(engine, model)?;
            }
        }
        Ok(())
    }

    pub fn get(&self, engine: Engine) -> Option<&str> {
        match engine {
            Engine::Claude => self.claude.as_deref(),
            Engine::Codex => self.codex.as_deref(),
            Engine::Opencode => self.opencode.as_deref(),
            Engine::Kimi => self.kimi.as_deref(),
            Engine::Grok => self.grok.as_deref(),
        }
    }

    pub fn set(&mut self, engine: Engine, model: String) {
        match engine {
            Engine::Claude => self.claude = Some(model),
            Engine::Codex => self.codex = Some(model),
            Engine::Opencode => self.opencode = Some(model),
            Engine::Kimi => self.kimi = Some(model),
            Engine::Grok => self.grok = Some(model),
        }
    }

    pub fn clear(&mut self, engine: Engine) {
        match engine {
            Engine::Claude => self.claude = None,
            Engine::Codex => self.codex = None,
            Engine::Opencode => self.opencode = None,
            Engine::Kimi => self.kimi = None,
            Engine::Grok => self.grok = None,
        }
    }

    pub fn effective(&self) -> Result<BTreeMap<String, EffectiveModel>> {
        let environment = std::env::vars().collect::<BTreeMap<_, _>>();
        self.effective_with_environment(&environment)
    }

    pub fn effective_with_environment(
        &self,
        environment: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, EffectiveModel>> {
        Engine::ALL
            .into_iter()
            .map(|engine| {
                self.effective_model_with_environment(engine, None, environment)
                    .map(|model| (engine.to_string(), model))
            })
            .collect()
    }

    pub fn effective_model(
        &self,
        engine: Engine,
        explicit: Option<&str>,
    ) -> Result<EffectiveModel> {
        let environment = std::env::vars().collect::<BTreeMap<_, _>>();
        self.effective_model_with_environment(engine, explicit, &environment)
    }

    pub fn effective_model_with_environment(
        &self,
        engine: Engine,
        explicit: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<EffectiveModel> {
        let config_model = self.get(engine);
        let mut resolved_environment = environment.clone();
        if let Some(value) = config_model {
            resolved_environment.insert(catalog::spec(engine).model_env, value.to_owned());
        }
        let resolved =
            catalog::resolve_model(engine, explicit, &MapEnvironment::new(resolved_environment))?;
        let source = if explicit.is_some() {
            "argv"
        } else if config_model.is_some() {
            "config"
        } else {
            match resolved.origin {
                ModelOrigin::Explicit => "argv",
                ModelOrigin::ProviderEnvironment | ModelOrigin::GlobalEnvironment => "environment",
                ModelOrigin::StrictDefault => "default",
            }
        };
        Ok(EffectiveModel {
            engine: engine.to_string(),
            model: resolved.id,
            default: catalog::resolve_model(engine, None, &MapEnvironment::default())?.id,
            source,
        })
    }
}

/// Resolve a user-supplied model ID without consulting ambient model settings.
///
/// Explicit IDs are validated and canonicalized here so every caller accepts
/// the same aliases and provider-specific shape, while still allowing any
/// non-empty model ID supported by the local provider CLI.
pub fn resolve_explicit_model(engine: Engine, model: &str) -> Result<String> {
    catalog::resolve_model(engine, Some(model), &MapEnvironment::default())
        .map(|resolved| resolved.id)
}

pub fn model_config_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(|parent| parent.join("models.toml"))
        .unwrap_or_else(|| PathBuf::from("models.toml"))
}

pub fn explicit_model_overrides(
    config_path: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<BTreeMap<Engine, String>> {
    let path = model_config_path(config_path);
    let settings = ModelOverrides::load(&path)?;
    Engine::ALL
        .into_iter()
        .filter_map(|engine| {
            let configured = settings.get(engine);
            let resolved = settings.effective_model_with_environment(engine, None, environment);
            match resolved {
                Ok(model) if configured.is_some() || model.source != "default" => {
                    Some(Ok((engine, model.model)))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

pub fn process_environment_model_overrides(config_path: &Path) -> Result<BTreeMap<Engine, String>> {
    let environment = std::env::vars().collect();
    explicit_model_overrides(config_path, &environment)
}

#[cfg(test)]
#[path = "models_tests/mod.rs"]
mod tests;
