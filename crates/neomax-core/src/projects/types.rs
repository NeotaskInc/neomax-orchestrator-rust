use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub root: PathBuf,
    #[serde(default = "default_repositories")]
    pub repos: Vec<PathBuf>,
    #[serde(default)]
    pub branch_prefix: Option<String>,
    #[serde(default)]
    pub brain: Option<PathBuf>,
    #[serde(default)]
    pub agents: Option<PathBuf>,
    #[serde(default)]
    pub orch_brain: Option<PathBuf>,
    #[serde(default)]
    pub opener: Option<PathBuf>,
    #[serde(default)]
    pub planning: Option<PathBuf>,
    #[serde(default, rename = "desc")]
    pub description: Option<String>,
    #[serde(default, rename = "created")]
    pub created_at: Option<i64>,
    #[serde(default)]
    pub auto_registered: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Project {
    pub fn portable(root: PathBuf, prefix: String, created_at: i64) -> Self {
        Self {
            root,
            repos: default_repositories(),
            branch_prefix: Some(prefix),
            brain: Some("CLAUDE.md".into()),
            agents: Some("AGENTS.md".into()),
            orch_brain: Some("docs/neomax-orchestrator/ORCHESTRATOR.md".into()),
            opener: Some("docs/neomax-orchestrator/ORCHESTRATOR_OPENER.md".into()),
            planning: Some("docs/neomax-orchestrator".into()),
            description: None,
            created_at: Some(created_at),
            auto_registered: true,
            extra: BTreeMap::new(),
        }
    }
}

fn default_repositories() -> Vec<PathBuf> {
    vec![PathBuf::from(".")]
}
