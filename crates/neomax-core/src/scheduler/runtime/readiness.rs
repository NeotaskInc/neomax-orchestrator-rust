use std::collections::BTreeMap;

use super::super::{DependencyGraph, PartState, Plan, PlanState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Waiting { dependencies: Vec<String> },
    Blocked { dependencies: Vec<String> },
    Finished,
    UnknownPart,
}

pub struct DependencyReadiness<'a> {
    graph: &'a DependencyGraph,
    states: &'a PlanState,
}

impl<'a> DependencyReadiness<'a> {
    pub fn new(graph: &'a DependencyGraph, states: &'a PlanState) -> Self {
        Self { graph, states }
    }

    pub fn evaluate(&self, id: &str) -> Readiness {
        let Some(dependencies) = self.graph.dependencies(id) else {
            return Readiness::UnknownPart;
        };
        let state = self.states.state(id).unwrap_or(PartState::Pending);
        if state.is_terminal() {
            return Readiness::Finished;
        }
        let failed = dependencies
            .iter()
            .filter(|dependency| {
                matches!(
                    self.states.state(dependency),
                    Some(
                        PartState::Failed
                            | PartState::Conflict
                            | PartState::Blocked
                            | PartState::Unknown,
                    )
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        if !failed.is_empty() {
            return Readiness::Blocked {
                dependencies: failed,
            };
        }
        let waiting = dependencies
            .iter()
            .filter(|dependency| self.states.state(dependency) != Some(PartState::Done))
            .cloned()
            .collect::<Vec<_>>();
        if !waiting.is_empty() {
            return Readiness::Waiting {
                dependencies: waiting,
            };
        }
        if state == PartState::Pending {
            Readiness::Ready
        } else {
            Readiness::Finished
        }
    }

    pub fn ready_ids(&self) -> Vec<String> {
        self.graph.ready_order(&self.states.states)
    }

    pub fn block_failed(&self, states: &mut PlanState) -> Vec<String> {
        states.block_failed_dependencies(self.graph)
    }

    pub fn pending_dependencies(&self, plan: &Plan) -> BTreeMap<String, Vec<String>> {
        plan.parts
            .iter()
            .filter_map(|part| match self.evaluate(&part.id) {
                Readiness::Waiting { dependencies } => Some((part.id.clone(), dependencies)),
                _ => None,
            })
            .collect()
    }
}
