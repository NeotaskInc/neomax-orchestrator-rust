use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueReservation {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub want: u32,
    #[serde(default)]
    pub granted: u32,
    #[serde(default)]
    pub batch: Option<String>,
    #[serde(default)]
    pub ts: f64,
    #[serde(default)]
    pub session: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueState {
    pub agent_budget: u32,
    pub task_budget: u32,
    pub queue: Vec<QueueReservation>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for QueueState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Self::from_value(value, default_agent_budget(), 0).map_err(D::Error::custom)
    }
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            agent_budget: default_agent_budget(),
            task_budget: 0,
            queue: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

impl QueueState {
    pub(crate) fn from_value(
        value: Value,
        agent_default: u32,
        task_default: u32,
    ) -> Result<Self, &'static str> {
        let Value::Object(mut object) = value else {
            return Err("queue state must be an object");
        };
        let agent_budget = parse_u32(object.remove("agent_budget"), agent_default);
        let task_budget = parse_u32(object.remove("task_budget"), task_default);
        let queue = object.remove("queue").map(parse_queue).unwrap_or_default();
        Ok(Self {
            agent_budget,
            task_budget,
            queue,
            extra: object.into_iter().collect(),
        })
    }

    pub(crate) fn with_budgets(agent_budget: u32, task_budget: u32) -> Self {
        Self {
            agent_budget,
            task_budget,
            ..Self::default()
        }
    }

    pub fn metrics(&self) -> QueueMetrics {
        let used = self
            .queue
            .iter()
            .map(|item| item.granted.min(item.want))
            .fold(0, u32::saturating_add);
        QueueMetrics {
            agent_budget: self.agent_budget,
            task_budget: self.task_budget,
            used,
            free: self.agent_budget.saturating_sub(used),
            active_tasks: self.queue.iter().filter(|item| item.granted != 0).count(),
            queued_tasks: self.queue.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueMetrics {
    pub agent_budget: u32,
    pub task_budget: u32,
    pub used: u32,
    pub free: u32,
    pub active_tasks: usize,
    pub queued_tasks: usize,
}

const fn default_agent_budget() -> u32 {
    50
}

fn parse_queue(value: Value) -> Vec<QueueReservation> {
    let Value::Array(entries) = value else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect()
}

fn parse_u32(value: Option<Value>, default: u32) -> u32 {
    value
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or(default)
}
