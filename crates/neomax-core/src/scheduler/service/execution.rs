use std::sync::Arc;

use super::super::runtime::{
    DispatchError, DispatchReceipt, DispatchRequest, DispatchResult, WorkerOutcome, WorkerRunner,
};
use crate::{Error, Result};

pub trait WorkerExecution: Send + Sync + 'static {
    fn dispatch(&self, request: DispatchRequest) -> Result<DispatchReceipt>;

    fn dispatch_classified(&self, request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        self.dispatch(request)
            .map_err(|error| DispatchError::terminal(error.to_string()))
    }

    fn poll(&self, run_id: &str) -> Result<Option<WorkerOutcome>>;

    fn cancel(&self, run_id: &str) -> Result<()>;
}

pub struct CoordinatorWorkerRunner<E> {
    execution: Arc<E>,
}

pub type ProviderWorkerRunner = CoordinatorWorkerRunner<super::provider_runner::ProviderExecution>;

impl<E> CoordinatorWorkerRunner<E> {
    pub fn new(execution: Arc<E>) -> Self {
        Self { execution }
    }

    pub fn execution(&self) -> &Arc<E> {
        &self.execution
    }
}

impl<E> WorkerRunner for CoordinatorWorkerRunner<E>
where
    E: WorkerExecution,
{
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt> {
        self.execution.dispatch(request)
    }

    fn dispatch_classified(&mut self, request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        self.execution.dispatch_classified(request)
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>> {
        self.execution.poll(run_id)
    }

    fn cancel(&mut self, run_id: &str) -> Result<()> {
        self.execution.cancel(run_id)
    }
}

pub fn outcome_error(run_id: &str, error: impl std::fmt::Display) -> WorkerOutcome {
    WorkerOutcome::Failed {
        run_id: run_id.to_owned(),
        error: error.to_string(),
    }
}

pub fn lock_error(name: &str) -> Error {
    Error::Message(format!("{name} lock poisoned"))
}
