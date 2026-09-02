use super::super::super::PartState;
use super::super::admission::AdmissionController;
use super::super::clock::Clock;
use super::super::dispatch::{DispatchPlanner, WorkerRunner};
use super::super::transitions::{PartTransition, apply_transition};
use super::model::RuntimeCoordinator;
use super::types::TickReport;
use crate::Result;

pub(super) fn block_remaining<R, A, C, P>(
    coordinator: &mut RuntimeCoordinator<R, A, C, P>,
    report: &mut TickReport,
) -> Result<()>
where
    R: WorkerRunner,
    A: AdmissionController,
    C: Clock,
    P: DispatchPlanner,
{
    let ids = coordinator
        .state
        .states
        .iter()
        .filter_map(|(id, state)| (*state == PartState::Pending).then_some(id.clone()))
        .collect::<std::collections::BTreeSet<_>>();
    for id in ids {
        apply_transition(
            &mut coordinator.state,
            &id,
            PartTransition::Block {
                dependencies: Vec::new(),
            },
        )?;
        report.blocked.push(id);
    }
    Ok(())
}
