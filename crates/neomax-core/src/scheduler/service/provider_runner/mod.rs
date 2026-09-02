mod config;
mod jobs;
mod outcome;
mod request;
mod run;

pub use config::ProviderExecutionConfig;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::super::runtime::{DispatchReceipt, DispatchRequest, DispatchResult, WorkerOutcome};
use super::execution::WorkerExecution;
use crate::concurrency::dispatch::DispatchAdmissionStore;
use crate::Result;

pub struct ProviderExecution {
    pub(super) inner: Arc<ProviderExecutionInner>,
}

pub(super) struct ProviderExecutionInner {
    pub(super) providers: Arc<crate::providers::ProviderRegistry>,
    pub(super) settings: Arc<crate::EffectiveSettings>,
    pub(super) paths: crate::StatePaths,
    pub(super) scope: crate::WorkerScope,
    pub(super) selection: crate::accounts::SelectionPolicy,
    pub(super) default_cooldown: std::time::Duration,
    pub(super) clock: Arc<dyn crate::runs::coordinator::RunClock>,
    pub(super) admission: DispatchAdmissionStore,
    pub(super) jobs: Mutex<BTreeMap<String, Job>>,
}

pub(super) struct Job {
    pub(super) internal_run_id: String,
    pub(super) handle: JoinHandle<Result<WorkerOutcome>>,
}

impl ProviderExecution {
    pub fn new(config: ProviderExecutionConfig) -> Result<Self> {
        config.paths.ensure_runtime_dirs()?;
        let admission = DispatchAdmissionStore::from_settings(
            config.paths.state.clone(),
            config.settings.as_ref(),
        )?;
        Ok(Self {
            inner: Arc::new(ProviderExecutionInner {
                providers: config.providers,
                settings: config.settings,
                paths: config.paths,
                scope: config.scope,
                selection: config.selection,
                default_cooldown: config.default_cooldown,
                clock: config.clock,
                admission,
                jobs: Mutex::new(BTreeMap::new()),
            }),
        })
    }

    pub fn providers(&self) -> &crate::providers::ProviderRegistry {
        self.inner.providers.as_ref()
    }

    pub fn paths(&self) -> &crate::StatePaths {
        &self.inner.paths
    }
}

impl WorkerExecution for ProviderExecution {
    fn dispatch(&self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.dispatch_request(request)
    }

    fn dispatch_classified(&self, request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        self.dispatch_request_classified(request)
    }

    fn poll(&self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        self.poll_request(run_id)
    }

    fn cancel(&self, run_id: &str) -> Result<()> {
        self.cancel_request(run_id)
    }
}

impl Clone for ProviderExecution {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) use request::resolve_scheduler_model;
