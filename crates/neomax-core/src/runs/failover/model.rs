use std::collections::BTreeMap;

use crate::Engine;

/// Supplies an explicit model selected for a target provider.
///
/// Returning `None` means that the provider's strict default should be used.
/// Model validation and config-file loading belong to the caller so failover
/// can remain independent of CLI settings and provider discovery.
pub trait ModelResolver: Send + Sync {
    fn model_for(&self, engine: Engine) -> Option<String>;
}

impl<F> ModelResolver for F
where
    F: Fn(Engine) -> Option<String> + Send + Sync,
{
    fn model_for(&self, engine: Engine) -> Option<String> {
        self(engine)
    }
}

impl ModelResolver for BTreeMap<Engine, String> {
    fn model_for(&self, engine: Engine) -> Option<String> {
        self.get(&engine).cloned()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoModelOverrides;

impl ModelResolver for NoModelOverrides {
    fn model_for(&self, _engine: Engine) -> Option<String> {
        None
    }
}
