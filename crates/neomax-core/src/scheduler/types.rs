use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::Engine;

#[derive(Debug, Clone, Deserialize)]
pub struct PlanSpec {
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub integration_branch: Option<String>,
    #[serde(default, rename = "plan")]
    pub plan_id: Option<String>,
    #[serde(default, deserialize_with = "null_as_empty_parts")]
    pub parts: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl PlanSpec {
    pub fn new(parts: Vec<Value>) -> Self {
        Self {
            repo: None,
            base: None,
            integration_branch: None,
            plan_id: None,
            parts,
            extra: BTreeMap::new(),
        }
    }
}

fn null_as_empty_parts<'de, D>(deserializer: D) -> Result<Vec<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<Vec<Value>>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub integration_branch: Option<String>,
    #[serde(default, rename = "plan")]
    pub plan_id: Option<String>,
    pub parts: Vec<Part>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Plan {
    pub fn part(&self, id: &str) -> Option<&Part> {
        self.parts.iter().find(|part| part.id == id)
    }

    pub fn part_ids(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().map(|part| part.id.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub prompt: String,
    #[serde(default = "default_part_engine")]
    pub engine: Engine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub area: BTreeSet<String>,
    #[serde(default, rename = "depends_on")]
    pub depends_on: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default)]
    pub ultra: bool,
    #[serde(default)]
    pub opus: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kimi_model: Option<String>,
    #[serde(default)]
    pub order: usize,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn default_part_engine() -> Engine {
    Engine::Claude
}

impl Part {
    pub fn areas(&self) -> impl Iterator<Item = &str> {
        self.area.iter().map(String::as_str)
    }

    pub fn dependencies(&self) -> impl Iterator<Item = &str> {
        self.depends_on.iter().map(String::as_str)
    }

    pub fn lock_areas(&self) -> Vec<&str> {
        if self.area.is_empty() {
            vec!["*"]
        } else {
            self.areas().collect()
        }
    }
}
