use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::{Engine, Error, Result};

use super::super::{Part, Plan};

/// The scheduler must be able to distinguish a worker that cannot start yet
/// from a worker request that can never be valid. Deferred failures stay in
/// the pending queue; terminal failures transition the part to failed.
#[derive(Debug)]
pub enum DispatchError {
    Deferred {
        reason: String,
        retry_at: Option<i64>,
    },
    Terminal {
        reason: String,
    },
}

pub type DispatchResult<T> = std::result::Result<T, DispatchError>;

impl DispatchError {
    pub fn deferred(reason: impl Into<String>) -> Self {
        Self::Deferred {
            reason: reason.into(),
            retry_at: None,
        }
    }

    pub fn deferred_until(reason: impl Into<String>, retry_at: i64) -> Self {
        Self::Deferred {
            reason: reason.into(),
            retry_at: Some(retry_at),
        }
    }

    pub fn terminal(reason: impl Into<String>) -> Self {
        Self::Terminal {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::Deferred { reason, .. } | Self::Terminal { reason } => reason,
        }
    }

    pub const fn retry_at(&self) -> Option<i64> {
        match self {
            Self::Deferred { retry_at, .. } => *retry_at,
            Self::Terminal { .. } => None,
        }
    }

    pub const fn is_deferred(&self) -> bool {
        matches!(self, Self::Deferred { .. })
    }

    pub fn into_error(self) -> Error {
        Error::Message(self.reason().to_owned())
    }
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchRequest {
    pub plan_id: String,
    pub part_id: String,
    pub run_id: String,
    pub attempt: u32,
    pub engine: Engine,
    pub model: Option<String>,
    pub prompt: String,
    pub areas: Vec<String>,
    pub dependencies: Vec<String>,
    pub cwd: PathBuf,
    pub repository: Option<PathBuf>,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub environment: BTreeMap<String, String>,
}

impl DispatchRequest {
    pub fn for_part(
        plan: &Plan,
        part: &Part,
        run_id: impl Into<String>,
        attempt: u32,
        cwd: impl Into<PathBuf>,
    ) -> Result<Self> {
        let plan_id = plan
            .plan_id
            .clone()
            .ok_or_else(|| Error::InvalidArgument("scheduler plan has no id".into()))?;
        let model = part
            .model
            .clone()
            .or_else(|| part.codex_model.clone())
            .or_else(|| part.kimi_model.clone());
        Ok(Self {
            plan_id,
            part_id: part.id.clone(),
            run_id: run_id.into(),
            attempt,
            engine: part.engine,
            model,
            prompt: part.prompt.clone(),
            areas: part.lock_areas().into_iter().map(str::to_owned).collect(),
            dependencies: part.dependencies().map(str::to_owned).collect(),
            cwd: cwd.into(),
            repository: plan.repo.clone(),
            branch: None,
            base: plan.base.clone(),
            environment: BTreeMap::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchReceipt {
    pub run_id: String,
    pub branch: Option<String>,
    pub profile: Option<String>,
    pub launched_at: i64,
}

impl DispatchReceipt {
    pub fn new(run_id: impl Into<String>, launched_at: i64) -> Self {
        Self {
            run_id: run_id.into(),
            branch: None,
            profile: None,
            launched_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerOutcome {
    Completed {
        run_id: String,
    },
    Failed {
        run_id: String,
        error: String,
    },
    RateLimited {
        run_id: String,
        retry_at: Option<i64>,
        error: Option<String>,
    },
    Conflict {
        run_id: String,
        error: String,
    },
    Interrupted {
        run_id: String,
        error: Option<String>,
    },
    Missing {
        run_id: String,
        error: Option<String>,
    },
}

impl WorkerOutcome {
    pub fn run_id(&self) -> &str {
        match self {
            Self::Completed { run_id }
            | Self::Failed { run_id, .. }
            | Self::RateLimited { run_id, .. }
            | Self::Conflict { run_id, .. }
            | Self::Interrupted { run_id, .. }
            | Self::Missing { run_id, .. } => run_id,
        }
    }
}

pub trait WorkerRunner {
    fn dispatch(&mut self, request: DispatchRequest) -> Result<DispatchReceipt>;

    fn dispatch_classified(&mut self, request: DispatchRequest) -> DispatchResult<DispatchReceipt> {
        self.dispatch(request)
            .map_err(|error| DispatchError::terminal(error.to_string()))
    }

    fn poll(&mut self, run_id: &str) -> Result<Option<WorkerOutcome>>;

    fn cancel(&mut self, _run_id: &str) -> Result<()> {
        Ok(())
    }
}

/// Polls a worker that was discovered after the scheduler process restarted.
///
/// A recovered worker is deliberately separate from `WorkerRunner`: the
/// runner owns newly launched jobs, while this handle owns the provider-native
/// identity used to observe an already-running job.
pub trait RecoveredWorker {
    fn poll(&mut self) -> Result<Option<WorkerOutcome>>;

    fn cancel(&mut self) -> Result<()> {
        Ok(())
    }
}

pub trait DispatchPlanner {
    fn plan(&self, plan: &Plan, part: &Part, attempt: u32) -> Result<DispatchRequest>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultDispatchPlanner {
    pub working_directory: PathBuf,
}

impl DefaultDispatchPlanner {
    pub fn new(working_directory: impl Into<PathBuf>) -> Self {
        Self {
            working_directory: working_directory.into(),
        }
    }
}

impl DispatchPlanner for DefaultDispatchPlanner {
    fn plan(&self, plan: &Plan, part: &Part, attempt: u32) -> Result<DispatchRequest> {
        let plan_id = plan
            .plan_id
            .as_deref()
            .ok_or_else(|| Error::InvalidArgument("scheduler plan has no id".into()))?;
        DispatchRequest::for_part(
            plan,
            part,
            format!("{plan_id}-{}", part.id),
            attempt,
            self.working_directory.clone(),
        )
    }
}
