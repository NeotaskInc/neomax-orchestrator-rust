use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use crate::atomic::{read_json, with_exclusive_lock, write_json_atomic};
use crate::settings::EffectiveSettings;
use crate::{Error, Result};

use super::allocation::allocate;
use super::liveness::{SessionLiveness, SessionState};
use super::types::{QueueReservation, QueueState};

pub struct AgentQueue {
    path: PathBuf,
    default_agent_budget: u32,
    default_task_budget: u32,
    reservation_ttl_seconds: f64,
}

impl AgentQueue {
    pub fn from_settings(path: impl Into<PathBuf>, settings: &EffectiveSettings) -> Self {
        Self::new(
            path,
            settings.concurrency.max_subagents,
            settings.concurrency.max_tasks,
            settings.concurrency.queue_ttl_seconds,
        )
    }

    pub fn new(
        path: impl Into<PathBuf>,
        default_agent_budget: u32,
        default_task_budget: u32,
        reservation_ttl_seconds: f64,
    ) -> Self {
        Self {
            path: path.into(),
            default_agent_budget,
            default_task_budget,
            reservation_ttl_seconds,
        }
    }

    pub fn reserve(
        &self,
        task: &str,
        agents: u32,
        session: &str,
        batch: Option<String>,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<QueueReservation> {
        if task.trim().is_empty() {
            return Err(Error::InvalidArgument("queue task is empty".into()));
        }
        let task = task.trim().to_string();
        let session = if session.is_empty() {
            format!("pid-{}", std::process::id())
        } else {
            session.into()
        };
        let (id, state) = self.transaction(now, liveness, |state| {
            if let Some(existing) = state.queue.iter_mut().find(|item| item.task == task) {
                existing.want = existing.want.max(agents);
                if batch.is_some() {
                    existing.batch = batch;
                }
                return Ok(existing.id.clone());
            }
            let id = format!("res-{}", Uuid::new_v4().simple());
            state.queue.push(QueueReservation {
                id: id.clone(),
                task,
                want: agents,
                granted: 0,
                batch,
                ts: now,
                session,
                extra: Default::default(),
            });
            Ok(id)
        })?;
        state
            .queue
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| Error::Message("queue reservation disappeared".into()))
    }

    pub fn poll(
        &self,
        id: Option<&str>,
        task: Option<&str>,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<Option<QueueReservation>> {
        let (_, state) = self.transaction(now, liveness, |_| Ok(()))?;
        Ok(state.queue.into_iter().find(|item| {
            id.is_some_and(|value| item.id == value) || task.is_some_and(|value| item.task == value)
        }))
    }

    pub fn release(
        &self,
        id: Option<&str>,
        task: Option<&str>,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<usize> {
        self.transaction(now, liveness, |state| {
            let before = state.queue.len();
            state.queue.retain(|item| {
                !(id.is_some_and(|value| item.id == value)
                    || task.is_some_and(|value| item.task == value))
            });
            Ok(before - state.queue.len())
        })
        .map(|(released, _)| released)
    }

    pub fn set_budgets(
        &self,
        agents: Option<u32>,
        tasks: Option<u32>,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> Result<QueueState> {
        self.transaction(now, liveness, |state| {
            if let Some(value) = agents {
                state.agent_budget = value;
            }
            if let Some(value) = tasks {
                state.task_budget = value;
            }
            Ok(())
        })
        .map(|(_, state)| state)
    }

    pub fn snapshot(&self, now: f64, liveness: &dyn SessionLiveness) -> Result<QueueState> {
        self.transaction(now, liveness, |_| Ok(()))
            .map(|(_, state)| state)
    }

    fn transaction<T>(
        &self,
        now: f64,
        liveness: &dyn SessionLiveness,
        operation: impl FnOnce(&mut QueueState) -> Result<T>,
    ) -> Result<(T, QueueState)> {
        with_exclusive_lock(&lock_path(&self.path), || {
            let mut state = self.load_state()?;
            state.queue.retain(|item| self.alive(item, now, liveness));
            let result = operation(&mut state)?;
            allocate(&mut state);
            write_json_atomic(&self.path, &state)?;
            Ok((result, state))
        })
    }

    fn load_state(&self) -> Result<QueueState> {
        match read_json::<Value>(&self.path) {
            Ok(value) => Ok(QueueState::from_value(
                value,
                self.default_agent_budget,
                self.default_task_budget,
            )
            .unwrap_or_else(|_| {
                QueueState::with_budgets(self.default_agent_budget, self.default_task_budget)
            })),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(
                QueueState::with_budgets(self.default_agent_budget, self.default_task_budget),
            ),
            // Queue state is an optional admission cache. Match the reference loader by
            // rebuilding its defaults when the file is missing a JSON object or is corrupt;
            // bounded-read and filesystem failures still fail closed.
            Err(Error::InvalidState { .. }) => Ok(QueueState::with_budgets(
                self.default_agent_budget,
                self.default_task_budget,
            )),
            Err(error) => Err(error),
        }
    }

    fn alive(
        &self,
        reservation: &QueueReservation,
        now: f64,
        liveness: &dyn SessionLiveness,
    ) -> bool {
        if !reservation.ts.is_finite() || now - reservation.ts > self.reservation_ttl_seconds {
            return false;
        }
        if reservation.session.starts_with("pid-") {
            return true;
        }
        liveness.state(&reservation.session) != SessionState::Dead
    }
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}
