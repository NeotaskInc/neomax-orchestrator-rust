use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{Error, Result};

use super::state::PartState;
use super::types::Part;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyGraph {
    dependencies: BTreeMap<String, BTreeSet<String>>,
    dependents: BTreeMap<String, BTreeSet<String>>,
    order: BTreeMap<String, usize>,
}

impl DependencyGraph {
    pub fn build(parts: &[Part]) -> Result<Self> {
        let mut dependencies = BTreeMap::new();
        let mut dependents = BTreeMap::new();
        let mut order = BTreeMap::new();
        for part in parts {
            if dependencies.contains_key(&part.id) {
                return Err(Error::InvalidArgument(format!(
                    "neomax run-all: duplicate part id {:?}",
                    part.id
                )));
            }
            dependencies.insert(part.id.clone(), part.depends_on.clone());
            dependents.insert(part.id.clone(), BTreeSet::new());
            order.insert(part.id.clone(), part.order);
        }
        for (part, required) in &dependencies {
            let unknown = required
                .iter()
                .filter(|dependency| !dependents.contains_key(*dependency))
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                return Err(Error::InvalidArgument(format!(
                    "neomax run-all: part {part} depends on unknown part(s): {}",
                    unknown.join(", ")
                )));
            }
            for dependency in required {
                let children = dependents
                    .get_mut(dependency)
                    .expect("unknown dependencies are rejected above");
                children.insert(part.clone());
            }
        }
        let graph = Self {
            dependencies,
            dependents,
            order,
        };
        graph.ensure_acyclic()?;
        Ok(graph)
    }

    pub fn dependencies(&self, id: &str) -> Option<&BTreeSet<String>> {
        self.dependencies.get(id)
    }

    pub fn dependents(&self, id: &str) -> Option<&BTreeSet<String>> {
        self.dependents.get(id)
    }

    pub fn direct_dependent_count(&self, id: &str) -> usize {
        self.dependents.get(id).map_or(0, BTreeSet::len)
    }

    pub fn ids(&self) -> Vec<&str> {
        let mut ids = self.order.iter().collect::<Vec<_>>();
        ids.sort_by_key(|(_, order)| **order);
        ids.into_iter().map(|(id, _)| id.as_str()).collect()
    }

    pub fn ready_order(&self, states: &BTreeMap<String, PartState>) -> Vec<String> {
        let mut ready = self
            .dependencies
            .iter()
            .filter_map(|(id, dependencies)| {
                (states.get(id).copied().unwrap_or(PartState::Pending) == PartState::Pending
                    && dependencies
                        .iter()
                        .all(|dependency| states.get(dependency).copied() == Some(PartState::Done)))
                .then_some(id.clone())
            })
            .collect::<Vec<_>>();
        ready.sort_by_key(|id| {
            (
                std::cmp::Reverse(self.direct_dependent_count(id)),
                self.order.get(id).copied().unwrap_or(usize::MAX),
            )
        });
        ready
    }

    pub fn blocked_by_failed_dependency(
        &self,
        id: &str,
        states: &BTreeMap<String, PartState>,
    ) -> bool {
        self.dependencies.get(id).is_some_and(|dependencies| {
            dependencies.iter().any(|dependency| {
                matches!(
                    states.get(dependency).copied(),
                    Some(
                        PartState::Failed
                            | PartState::Conflict
                            | PartState::Blocked
                            | PartState::Unknown,
                    )
                )
            })
        })
    }

    pub fn failed_dependency_ids(
        &self,
        id: &str,
        states: &BTreeMap<String, PartState>,
    ) -> Vec<String> {
        self.dependencies
            .get(id)
            .into_iter()
            .flat_map(|dependencies| dependencies.iter())
            .filter(|dependency| {
                matches!(
                    states.get(*dependency).copied(),
                    Some(
                        PartState::Failed
                            | PartState::Conflict
                            | PartState::Blocked
                            | PartState::Unknown,
                    )
                )
            })
            .cloned()
            .collect()
    }

    fn ensure_acyclic(&self) -> Result<()> {
        let mut indegree = self
            .dependencies
            .iter()
            .map(|(id, dependencies)| (id.clone(), dependencies.len()))
            .collect::<BTreeMap<_, _>>();
        let mut queue = VecDeque::from_iter(
            indegree
                .iter()
                .filter_map(|(id, degree)| (*degree == 0).then_some(id.clone())),
        );
        let mut seen = 0;
        while let Some(id) = queue.pop_front() {
            seen += 1;
            if let Some(children) = self.dependents.get(&id) {
                for child in children {
                    let degree = indegree
                        .get_mut(child)
                        .expect("dependency graph contains every dependent");
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(child.clone());
                    }
                }
            }
        }
        if seen != self.dependencies.len() {
            return Err(Error::InvalidArgument(
                "neomax run-all: dependency CYCLE detected: cannot schedule".into(),
            ));
        }
        Ok(())
    }
}
