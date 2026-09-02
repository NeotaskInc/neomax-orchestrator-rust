use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::atomic::{read_json_or_default, update_json_locked};
use crate::{Engine, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSelection {
    pub engine: Engine,
    pub selected_at: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub projects: BTreeMap<String, ProjectSelection>,
    pub extra: BTreeMap<String, serde_json::Value>,
    #[doc(hidden)]
    pub unknown_projects: BTreeMap<String, Value>,
}

impl Serialize for SelectionState {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut root = self
            .extra
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>();
        let mut projects = self
            .projects
            .iter()
            .map(|(key, value)| {
                serde_json::to_value(value)
                    .map(|value| (key.clone(), value))
                    .map_err(serde::ser::Error::custom)
            })
            .collect::<std::result::Result<Map<_, _>, _>>()?;
        for (key, value) in &self.unknown_projects {
            projects.entry(key.clone()).or_insert_with(|| value.clone());
        }
        root.insert("projects".into(), Value::Object(projects));
        Value::Object(root).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SelectionState {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| D::Error::custom("selection state must be a JSON object"))?;
        let mut projects = BTreeMap::new();
        let mut unknown_projects = BTreeMap::new();
        if let Some(entries) = object.get("projects").and_then(Value::as_object) {
            for (key, value) in entries {
                match serde_json::from_value::<ProjectSelection>(value.clone()) {
                    Ok(selection) => {
                        projects.insert(key.clone(), selection);
                    }
                    Err(_) => {
                        unknown_projects.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        let extra = object
            .iter()
            .filter(|(key, _)| key.as_str() != "projects")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Self {
            projects,
            extra,
            unknown_projects,
        })
    }
}

pub struct SelectionStateStore {
    path: PathBuf,
    lock: PathBuf,
}

impl SelectionStateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let lock = PathBuf::from(format!("{}.lock", path.to_string_lossy()));
        Self { path, lock }
    }

    pub fn load(&self) -> SelectionState {
        read_json_or_default(&self.path)
    }

    pub fn previous_engine(&self, project: &Path) -> Option<Engine> {
        self.load()
            .projects
            .get(&project_key(project))
            .map(|selection| selection.engine)
    }

    pub fn record(&self, project: &Path, engine: Engine, now: i64) -> Result<SelectionState> {
        update_json_locked(&self.path, &self.lock, |state: &mut SelectionState| {
            let key = project_key(project);
            let mut extra = state
                .projects
                .get(&key)
                .map(|selection| selection.extra.clone())
                .unwrap_or_default();
            if let Some(value) = state.unknown_projects.remove(&key) {
                if let Some(fields) = value.as_object() {
                    for (field, value) in fields {
                        if field != "engine" && field != "selected_at" {
                            extra.insert(field.clone(), value.clone());
                        }
                    }
                }
            }
            state.projects.insert(
                key,
                ProjectSelection {
                    engine,
                    selected_at: now,
                    extra,
                },
            );
            Ok(())
        })
    }
}

fn project_key(project: &Path) -> String {
    project.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_project_affinity_without_discarding_other_projects() {
        let temp = tempfile::tempdir().unwrap();
        let store = SelectionStateStore::new(temp.path().join("selection.json"));
        store
            .record(Path::new("/workspace/one"), Engine::Codex, 10)
            .unwrap();
        store
            .record(Path::new("/workspace/two"), Engine::Kimi, 20)
            .unwrap();
        assert_eq!(
            store.previous_engine(Path::new("/workspace/one")),
            Some(Engine::Codex)
        );
        assert_eq!(store.load().projects.len(), 2);
    }

    #[test]
    fn preserves_future_root_project_fields_and_engines_without_selecting_unknowns() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("selection.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "projects": {
                    "/workspace/known": {
                        "engine": "codex",
                        "selected_at": 10,
                        "future_project_field": {"enabled": true}
                    },
                    "/workspace/future": {
                        "engine": "future-engine",
                        "selected_at": 20,
                        "future_project_field": "preserve"
                    }
                },
                "future_root_field": ["preserve"]
            })
            .to_string(),
        )
        .unwrap();

        let store = SelectionStateStore::new(&path);
        let state = store.load();
        assert_eq!(
            store.previous_engine(Path::new("/workspace/known")),
            Some(Engine::Codex)
        );
        assert_eq!(store.previous_engine(Path::new("/workspace/future")), None);
        assert_eq!(
            state.projects["/workspace/known"].extra["future_project_field"],
            serde_json::json!({"enabled": true})
        );
        let round_trip = serde_json::to_value(&state).unwrap();
        assert_eq!(
            round_trip["future_root_field"],
            serde_json::json!(["preserve"])
        );
        assert_eq!(
            round_trip["projects"]["/workspace/future"]["engine"],
            "future-engine"
        );
        assert_eq!(
            round_trip["projects"]["/workspace/future"]["future_project_field"],
            "preserve"
        );
    }

    #[test]
    fn isolates_malformed_projects_and_preserves_them_when_another_project_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("selection.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "projects": {
                    "/workspace/good": {"engine": "claude", "selected_at": 1},
                    "/workspace/bad": {"selected_at": "not-a-number"}
                },
                "future_root_field": true
            })
            .to_string(),
        )
        .unwrap();

        let store = SelectionStateStore::new(&path);
        assert_eq!(
            store.previous_engine(Path::new("/workspace/good")),
            Some(Engine::Claude)
        );
        assert_eq!(store.previous_engine(Path::new("/workspace/bad")), None);
        store
            .record(Path::new("/workspace/new"), Engine::Grok, 3)
            .unwrap();

        let value: Value = crate::atomic::read_json(&path).unwrap();
        assert_eq!(value["future_root_field"], true);
        assert_eq!(value["projects"]["/workspace/good"]["engine"], "claude");
        assert_eq!(
            value["projects"]["/workspace/bad"]["selected_at"],
            "not-a-number"
        );
        assert_eq!(value["projects"]["/workspace/new"]["engine"], "grok");
    }

    #[test]
    fn recording_known_project_keeps_future_project_fields() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("selection.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "projects": {
                    "/workspace/project": {
                        "engine": "future-engine",
                        "selected_at": 2,
                        "future": {"value": 9}
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        SelectionStateStore::new(&path)
            .record(Path::new("/workspace/project"), Engine::Opencode, 4)
            .unwrap();
        let value: Value = crate::atomic::read_json(&path).unwrap();
        assert_eq!(
            value["projects"]["/workspace/project"]["engine"],
            "opencode"
        );
        assert_eq!(value["projects"]["/workspace/project"]["selected_at"], 4);
        assert_eq!(
            value["projects"]["/workspace/project"]["future"]["value"],
            9
        );
    }
}
