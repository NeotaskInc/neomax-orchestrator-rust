use std::sync::Arc;
use std::time::Duration;

use crate::accounts::SelectionPolicy;
use crate::providers::ProviderRegistry;
use crate::runs::coordinator::{RunClock, SystemClock};
use crate::{EffectiveSettings, StatePaths, WorkerScope};

pub struct ProviderExecutionConfig {
    pub providers: Arc<ProviderRegistry>,
    pub settings: Arc<EffectiveSettings>,
    pub paths: StatePaths,
    pub scope: WorkerScope,
    pub selection: SelectionPolicy,
    pub default_cooldown: Duration,
    pub clock: Arc<dyn RunClock>,
}

impl ProviderExecutionConfig {
    pub fn new(
        providers: Arc<ProviderRegistry>,
        settings: Arc<EffectiveSettings>,
        paths: StatePaths,
    ) -> Self {
        let selection = SelectionPolicy::from_settings(settings.as_ref());
        Self {
            providers,
            settings,
            paths,
            scope: WorkerScope::all(),
            selection,
            default_cooldown: Duration::from_secs(30 * 60),
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_scope(mut self, scope: WorkerScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_selection(mut self, selection: SelectionPolicy) -> Self {
        self.selection = selection;
        self
    }

    pub fn with_default_cooldown(mut self, cooldown: Duration) -> Self {
        self.default_cooldown = cooldown;
        self
    }

    pub fn with_clock(mut self, clock: Arc<dyn RunClock>) -> Self {
        self.clock = clock;
        self
    }
}
