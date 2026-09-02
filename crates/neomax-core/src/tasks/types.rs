use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _, ser::Error as _};
use serde_json::Value;

use super::TaskStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: TaskStatus,
    pub created: i64,
    pub updated: i64,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub runs: Vec<String>,
    #[serde(flatten)]
    pub data: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRegistry {
    pub seq: u64,
    pub tasks: BTreeMap<String, Task>,
    pub extra: BTreeMap<String, Value>,
    pub invalid_tasks: BTreeMap<String, Value>,
}

impl Serialize for TaskRegistry {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tasks = BTreeMap::new();
        for (id, task) in &self.tasks {
            tasks.insert(
                id.clone(),
                serde_json::to_value(task).map_err(S::Error::custom)?,
            );
        }
        for (id, task) in &self.invalid_tasks {
            tasks.entry(id.clone()).or_insert_with(|| task.clone());
        }

        let mut object = self.extra.clone();
        object.insert("seq".into(), self.seq.into());
        object.insert("tasks".into(), serde_json::to_value(tasks).map_err(S::Error::custom)?);
        object.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TaskRegistry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let Value::Object(mut object) = Value::deserialize(deserializer)? else {
            return Err(D::Error::custom("task registry must be an object"));
        };

        let seq = object
            .remove("seq")
            .and_then(|value| value.as_u64())
            .unwrap_or_default();
        let mut tasks = BTreeMap::new();
        let mut invalid_tasks = BTreeMap::new();
        match object.remove("tasks") {
            Some(Value::Object(entries)) => {
                for (id, value) in entries {
                    match serde_json::from_value::<Task>(value.clone()) {
                        Ok(task) => {
                            tasks.insert(id, task);
                        }
                        Err(_) => {
                            invalid_tasks.insert(id, value);
                        }
                    }
                }
            }
            Some(_) => {
                return Err(D::Error::custom("task registry tasks must be an object"));
            }
            None => {}
        }

        Ok(Self {
            seq,
            tasks,
            extra: object.into_iter().collect(),
            invalid_tasks,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub status: Option<TaskStatus>,
    pub title: Option<String>,
    pub project: Option<Option<String>>,
    pub note: Option<String>,
    pub run_id: Option<String>,
}
