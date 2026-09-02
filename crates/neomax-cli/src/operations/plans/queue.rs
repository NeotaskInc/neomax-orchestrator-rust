use std::path::PathBuf;

use neomax_core::queue::{AgentQueue, QueueReservation, QueueState, SessionLiveness};
use neomax_core::{EffectiveSettings, Result};

pub(crate) struct PlanQueueBridge {
    queue: AgentQueue,
}

impl PlanQueueBridge {
    pub(crate) fn new(path: impl Into<PathBuf>, settings: &EffectiveSettings) -> Self {
        Self {
            queue: AgentQueue::from_settings(path, settings),
        }
    }

    pub(crate) fn reserve(
        &self,
        plan_id: &str,
        agents: u32,
        session: &str,
        batch: Option<String>,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<QueueLease> {
        let reservation =
            self.queue
                .reserve(&plan_task(plan_id), agents, session, batch, now, liveness)?;
        Ok(QueueLease { reservation })
    }

    pub(crate) fn poll(
        &self,
        lease: &QueueLease,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<Option<QueueReservation>> {
        self.queue
            .poll(Some(&lease.reservation.id), None, now, liveness)
    }

    pub(crate) fn release(
        &self,
        lease: &QueueLease,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<usize> {
        self.queue
            .release(Some(&lease.reservation.id), None, now, liveness)
    }

    pub(crate) fn snapshot(&self, now: f64, liveness: &dyn SessionLiveness) -> Result<QueueState> {
        self.queue.snapshot(now, liveness)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QueueLease {
    pub reservation: QueueReservation,
}

fn plan_task(plan_id: &str) -> String {
    format!("scheduler:{plan_id}")
}
