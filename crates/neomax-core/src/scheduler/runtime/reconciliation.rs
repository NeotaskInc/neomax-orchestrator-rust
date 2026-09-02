use super::dispatch::WorkerOutcome;
use super::transitions::PartTransition;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconciliation {
    pub transition: PartTransition,
    pub retry_at: Option<i64>,
}

pub fn reconcile(outcome: &WorkerOutcome) -> Reconciliation {
    match outcome {
        WorkerOutcome::Completed { .. } => Reconciliation {
            transition: PartTransition::Complete,
            retry_at: None,
        },
        WorkerOutcome::Failed { error, .. } => Reconciliation {
            transition: PartTransition::Fail {
                error: error.clone(),
            },
            retry_at: None,
        },
        WorkerOutcome::RateLimited {
            retry_at, error, ..
        } => Reconciliation {
            transition: PartTransition::Retry {
                reason: error
                    .clone()
                    .unwrap_or_else(|| "provider usage limit reached".into()),
            },
            retry_at: *retry_at,
        },
        WorkerOutcome::Conflict { error, .. } => Reconciliation {
            transition: PartTransition::Conflict {
                error: error.clone(),
            },
            retry_at: None,
        },
        WorkerOutcome::Interrupted { error, .. } => Reconciliation {
            transition: PartTransition::Retry {
                reason: error.clone().unwrap_or_else(|| "worker interrupted".into()),
            },
            retry_at: None,
        },
        WorkerOutcome::Missing { error, .. } => Reconciliation {
            transition: PartTransition::Retry {
                reason: error
                    .clone()
                    .unwrap_or_else(|| "worker disappeared before completion".into()),
            },
            retry_at: None,
        },
    }
}
