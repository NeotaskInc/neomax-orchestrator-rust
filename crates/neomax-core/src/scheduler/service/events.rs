use std::collections::BTreeMap;

use serde_json::Value;

use super::super::persistence::PlanEvent;
use super::ports::PersistencePort;
use crate::Result;

pub fn event(
    plan_id: &str,
    name: &str,
    timestamp: i64,
    part_id: Option<&str>,
    error: Option<&str>,
) -> Result<PlanEvent> {
    let mut result = PlanEvent::new(plan_id, name, timestamp)?;
    result.part_id = part_id.map(str::to_owned);
    result.error = error.map(str::to_owned);
    Ok(result)
}

pub fn event_with_fields(
    plan_id: &str,
    name: &str,
    timestamp: i64,
    part_id: Option<&str>,
    fields: impl IntoIterator<Item = (String, Value)>,
) -> Result<PlanEvent> {
    let mut result = event(plan_id, name, timestamp, part_id, None)?;
    result.extra = fields.into_iter().collect::<BTreeMap<_, _>>();
    Ok(result)
}

pub fn append<P: PersistencePort>(
    persistence: &P,
    plan_id: &str,
    name: &str,
    timestamp: i64,
    part_id: Option<&str>,
) -> Result<()> {
    persistence.append_event(&event(plan_id, name, timestamp, part_id, None)?)
}
