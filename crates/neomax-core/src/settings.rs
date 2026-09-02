mod capacity;
mod constants;
mod environment;
mod persistence;
mod schema;
mod validation;

mod models;

pub use constants::{
    DEFAULT_FLEET_LIVE_CAP, DEFAULT_QUEUE_TTL_SECONDS, FLEET_CAP_ENV, LANES_PER_ACCOUNT_ENV,
    LEGACY_AGENT_BUDGET_ENV, LEGACY_FLEET_CAP_ENV, LEGACY_LANES_PER_ACCT_ENV, LEGACY_QUEUE_TTL_ENV,
    LEGACY_TASK_BUDGET_ENV, MAX_LIVE_ENV, MAX_SESSIONS_PER_ACCOUNT_ENV, MAX_SUBAGENTS_ENV,
    MAX_TASKS_ENV, QUEUE_TTL_SECONDS_ENV,
};
pub use models::{
    explicit_model_overrides, model_config_path, process_environment_model_overrides,
    resolve_explicit_model, EffectiveModel, ModelOverrides,
};
pub use schema::{ConcurrencySettings, EffectiveSettings, SettingsFile};

#[cfg(test)]
#[path = "settings/tests/mod.rs"]
mod tests;
