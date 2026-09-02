use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::types::{SelfHealRecord, SelfHealState};

impl<'de> Deserialize<'de> for SelfHealState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let mut state = SelfHealState::default();

        for (key, value) in object {
            if key == "runs" {
                if let Some(records) = value.as_object() {
                    state.wrapped = true;
                    for (run_id, record) in records {
                        if let Ok(record) = serde_json::from_value::<SelfHealRecord>(record.clone())
                        {
                            state.runs.insert(run_id.clone(), record);
                        } else {
                            state.extra.insert(key.clone(), value.clone());
                            break;
                        }
                    }
                    continue;
                }
            }
            if let Ok(record) = serde_json::from_value::<SelfHealRecord>(value.clone()) {
                if value.is_object()
                    && (value.get("attempts").is_some()
                        || value.get("history").is_some()
                        || value.get("next_at").is_some()
                        || value.get("last_at").is_some())
                {
                    state.runs.insert(key, record);
                    continue;
                }
            }
            state.extra.insert(key, value);
        }
        Ok(state)
    }
}

impl Serialize for SelfHealState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object = BTreeMap::<String, serde_json::Value>::new();
        for (key, value) in &self.extra {
            object.insert(key.clone(), value.clone());
        }
        let mut runs = serde_json::Map::new();
        for (key, value) in &self.runs {
            let record = serde_json::to_value(value).map_err(serde::ser::Error::custom)?;
            if self.wrapped {
                runs.insert(key.clone(), record);
            } else {
                object.insert(key.clone(), record);
            }
        }
        if self.wrapped {
            object.insert("runs".into(), serde_json::Value::Object(runs));
        }
        object.serialize(serializer)
    }
}
