use std::collections::BTreeMap;

use anyhow::Result;
use neomax_core::Engine;

use crate::models::{EffectiveModel, ModelOverrides};

pub(crate) fn effective_models(
    overrides: &ModelOverrides,
    provider_models: &BTreeMap<Engine, String>,
    generic_model: Option<&str>,
    orchestrator: Option<Engine>,
) -> Result<BTreeMap<String, EffectiveModel>> {
    Engine::ALL
        .into_iter()
        .map(|engine| {
            let explicit = provider_models
                .get(&engine)
                .map(String::as_str)
                .or_else(|| {
                    (orchestrator.is_none() || Some(engine) == orchestrator)
                        .then_some(generic_model)
                        .flatten()
                });
            overrides
                .effective_model(engine, explicit)
                .map(|model| (engine.to_string(), model))
                .map_err(Into::into)
        })
        .collect()
}
