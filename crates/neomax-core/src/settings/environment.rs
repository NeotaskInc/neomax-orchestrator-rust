use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use crate::Result;

use super::constants::{
    FLEET_CAP_ENV, LANES_PER_ACCOUNT_ENV, LEGACY_AGENT_BUDGET_ENV, LEGACY_LANES_PER_ACCT_ENV,
    LEGACY_QUEUE_TTL_ENV, LEGACY_TASK_BUDGET_ENV, MAX_LIVE_ENV, MAX_SESSIONS_PER_ACCOUNT_ENV,
    MAX_SUBAGENTS_ENV, MAX_TASKS_ENV, QUEUE_TTL_SECONDS_ENV,
};
use super::schema::{EffectiveSettings, SettingsFile};
use super::validation::{
    first_environment_value, parse_non_negative, parse_positive, parse_positive_seconds,
    validate_concurrency,
};

impl EffectiveSettings {
    pub fn discover() -> Result<Self> {
        let path = SettingsFile::discover_path()?;
        let file = SettingsFile::load(&path)?;
        let values = env::vars().collect();
        Self::resolve(file, path, &values)
    }

    pub fn resolve(
        file: SettingsFile,
        config_path: PathBuf,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self> {
        validate_concurrency(&file.concurrency)?;
        let mut concurrency = file.concurrency;
        let mut source = config_path.display().to_string();
        if let Some((key, raw)) =
            first_environment_value(environment, [MAX_SUBAGENTS_ENV, LEGACY_AGENT_BUDGET_ENV])
        {
            concurrency.max_subagents = parse_positive(key, raw)?;
            source = key.into();
        }
        if let Some((key, raw)) =
            first_environment_value(environment, [MAX_LIVE_ENV, FLEET_CAP_ENV])
        {
            concurrency.fleet_live_cap = Some(parse_non_negative(key, raw)?);
        }
        if let Some((key, raw)) = first_environment_value(
            environment,
            [MAX_SESSIONS_PER_ACCOUNT_ENV, "NEOMAX_LIVE_CAP"],
        ) {
            concurrency.max_sessions_per_account = parse_positive(key, raw)?;
        }
        if let Some((key, raw)) =
            first_environment_value(environment, [MAX_TASKS_ENV, LEGACY_TASK_BUDGET_ENV])
        {
            concurrency.max_tasks = parse_non_negative(key, raw)?;
        }
        if let Some((key, raw)) = first_environment_value(
            environment,
            [LANES_PER_ACCOUNT_ENV, LEGACY_LANES_PER_ACCT_ENV],
        ) {
            concurrency.lanes_per_account = parse_positive(key, raw)?;
        }
        if let Some((key, raw)) =
            first_environment_value(environment, [QUEUE_TTL_SECONDS_ENV, LEGACY_QUEUE_TTL_ENV])
        {
            concurrency.queue_ttl_seconds = parse_positive_seconds(key, raw)?;
        }
        validate_concurrency(&concurrency)?;
        Ok(Self {
            concurrency,
            config_path,
            max_subagents_source: source,
        })
    }

    pub fn agent_environment(&self) -> BTreeMap<String, String> {
        let mut environment = BTreeMap::from([
            (
                MAX_SUBAGENTS_ENV.into(),
                self.concurrency.max_subagents.to_string(),
            ),
            (MAX_TASKS_ENV.into(), self.concurrency.max_tasks.to_string()),
            (
                MAX_SESSIONS_PER_ACCOUNT_ENV.into(),
                self.concurrency.max_sessions_per_account.to_string(),
            ),
            (
                LANES_PER_ACCOUNT_ENV.into(),
                self.concurrency.lanes_per_account.to_string(),
            ),
            (
                QUEUE_TTL_SECONDS_ENV.into(),
                self.concurrency.queue_ttl_seconds.to_string(),
            ),
        ]);
        if let Some(cap) = self.concurrency.fleet_live_cap {
            environment.insert(MAX_LIVE_ENV.into(), cap.to_string());
        }
        environment
    }
}
