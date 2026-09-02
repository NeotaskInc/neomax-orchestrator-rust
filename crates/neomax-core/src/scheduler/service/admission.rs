use std::sync::{Arc, Mutex};

use serde_json::json;

use super::super::runtime::{AdmissionController, AdmissionDecision, Clock, DispatchRequest};
use super::events::{append, event_with_fields};
use super::ports::PersistencePort;

#[derive(Clone, Default)]
pub struct ErrorState(Arc<Mutex<Option<String>>>);

impl ErrorState {
    pub fn take(&self) -> Option<String> {
        self.0.lock().ok().and_then(|mut value| value.take())
    }

    fn set(&self, error: impl Into<String>) {
        if let Ok(mut value) = self.0.lock() {
            *value = Some(error.into());
        }
    }
}

pub struct PersistentAdmission<A, P, C> {
    inner: A,
    persistence: Arc<P>,
    clock: C,
    errors: ErrorState,
}

impl<A, P, C> PersistentAdmission<A, P, C> {
    pub fn new(inner: A, persistence: Arc<P>, clock: C) -> Self {
        Self {
            inner,
            persistence,
            clock,
            errors: ErrorState::default(),
        }
    }

    pub fn errors(&self) -> ErrorState {
        self.errors.clone()
    }
}

impl<A, P, C> AdmissionController for PersistentAdmission<A, P, C>
where
    A: AdmissionController,
    P: PersistencePort,
    C: Clock + Clone,
{
    fn admit(&mut self, request: &DispatchRequest, active: usize) -> AdmissionDecision {
        if let Err(error) = append(
            self.persistence.as_ref(),
            &request.plan_id,
            "area_acquire_requested",
            self.clock.now(),
            Some(&request.part_id),
        ) {
            self.errors.set(error.to_string());
            return AdmissionDecision::AreaBusy {
                areas: request.areas.clone(),
            };
        }
        let decision = self.inner.admit(request, active);
        let name = if decision.admitted() {
            "area_acquired"
        } else {
            "area_unavailable"
        };
        let after = match event_with_fields(
            &request.plan_id,
            name,
            self.clock.now(),
            Some(&request.part_id),
            [("areas".into(), json!(request.areas))],
        ) {
            Ok(event) => event,
            Err(error) => {
                if decision.admitted() {
                    self.inner.release(request);
                }
                self.errors.set(error.to_string());
                return AdmissionDecision::AreaBusy {
                    areas: request.areas.clone(),
                };
            }
        };
        if let Err(error) = self.persistence.append_event(&after) {
            if decision.admitted() {
                self.inner.release(request);
            }
            self.errors.set(error.to_string());
            return AdmissionDecision::AreaBusy {
                areas: request.areas.clone(),
            };
        }
        decision
    }

    fn release(&mut self, request: &DispatchRequest) {
        if let Err(error) = append(
            self.persistence.as_ref(),
            &request.plan_id,
            "area_release_requested",
            self.clock.now(),
            Some(&request.part_id),
        ) {
            self.errors.set(error.to_string());
            self.inner.release(request);
            return;
        }
        self.inner.release(request);
        if let Err(error) = append(
            self.persistence.as_ref(),
            &request.plan_id,
            "area_released",
            self.clock.now(),
            Some(&request.part_id),
        ) {
            self.errors.set(error.to_string());
        }
    }

    fn release_after_cancel(&mut self, request: &DispatchRequest) {
        if let Err(error) = append(
            self.persistence.as_ref(),
            &request.plan_id,
            "area_release_requested",
            self.clock.now(),
            Some(&request.part_id),
        ) {
            self.errors.set(error.to_string());
            self.inner.release_after_cancel(request);
            return;
        }
        self.inner.release_after_cancel(request);
        if let Err(error) = append(
            self.persistence.as_ref(),
            &request.plan_id,
            "area_released",
            self.clock.now(),
            Some(&request.part_id),
        ) {
            self.errors.set(error.to_string());
        }
    }
}
