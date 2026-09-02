use serde_json::Value;

use crate::{Result, WorkerScope};

use super::graph::DependencyGraph;
use super::types::{Part, Plan, PlanSpec};
use super::validation::normalize_part;

impl Plan {
    pub fn normalize(spec: PlanSpec, scope: &WorkerScope) -> Result<Self> {
        if spec.parts.is_empty() {
            return Err(crate::Error::InvalidArgument(
                "neomax run-all: plan has no parts".into(),
            ));
        }
        let parts = spec
            .parts
            .iter()
            .enumerate()
            .map(|(index, value)| normalize_part(value, index, scope))
            .collect::<Result<Vec<_>>>()?;
        DependencyGraph::build(&parts)?;
        Ok(Self {
            repo: spec.repo,
            base: spec.base,
            integration_branch: spec.integration_branch,
            plan_id: spec.plan_id,
            parts,
            extra: spec.extra,
        })
    }

    pub fn from_value(value: Value, scope: &WorkerScope) -> Result<Self> {
        let spec: PlanSpec = serde_json::from_value(value).map_err(|error| {
            crate::Error::InvalidArgument(format!("neomax run-all: invalid plan: {error}"))
        })?;
        Self::normalize(spec, scope)
    }

    pub fn from_json(json: &str, scope: &WorkerScope) -> Result<Self> {
        let value = serde_json::from_str(json).map_err(|error| {
            crate::Error::InvalidArgument(format!("neomax run-all: cannot parse plan: {error}"))
        })?;
        Self::from_value(value, scope)
    }

    pub fn graph(&self) -> Result<DependencyGraph> {
        DependencyGraph::build(&self.parts)
    }

    pub fn from_parts(mut parts: Vec<Part>) -> Result<Self> {
        if parts.is_empty() {
            return Err(crate::Error::InvalidArgument(
                "neomax run-all: plan has no parts".into(),
            ));
        }
        for (order, part) in parts.iter_mut().enumerate() {
            part.order = order;
        }
        DependencyGraph::build(&parts)?;
        Ok(Self {
            repo: None,
            base: None,
            integration_branch: None,
            plan_id: None,
            parts,
            extra: Default::default(),
        })
    }
}
