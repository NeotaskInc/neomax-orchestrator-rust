mod adapters;
mod admission;
mod events;
mod execution;
mod lifecycle;
mod model;
mod persistence;
mod planner;
mod ports;
mod provider_runner;
mod recovery;
mod runner;
mod side_effects;
mod start;
mod sync;
mod workspace;

pub use adapters::{
    run_store_admission, shared_ttl_scheduler_admission, system_coordinator_recovery,
    ttl_scheduler_admission, CoordinatorRecovery, RunStoreSchedulerAdmission,
    SharedTtlSchedulerAdmission, SystemCoordinatorRecovery, TtlSchedulerAdmission,
};
pub use execution::{CoordinatorWorkerRunner, ProviderWorkerRunner, WorkerExecution};
pub use model::{AttachOptions, Coordinator, RunAllService, RunAllSpec};
pub use persistence::FilePlanPersistence;
pub use ports::{PersistencePort, RecoveryPort, RecoveryStatus, WorkspacePort};
pub use provider_runner::{ProviderExecution, ProviderExecutionConfig};
pub use recovery::{recover_running_parts, RecoveryReport};
pub use side_effects::{DurableDispatchPlanner, ErrorState, PersistentAdmission, PersistentRunner};

#[cfg(test)]
mod durability_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod provider_runner_tests;
#[cfg(test)]
mod recovery_adapter_tests;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod test_support;
