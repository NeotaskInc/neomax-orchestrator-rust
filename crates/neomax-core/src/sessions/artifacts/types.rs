use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    ClaudeMain,
    ClaudeSubagent,
    CodexRollout,
    OpenCodeDatabase,
    KimiState,
    KimiWire,
    GrokSummary,
    GrokUpdates,
}

impl ArtifactKind {
    pub const ALL: [Self; 8] = [
        Self::ClaudeMain,
        Self::ClaudeSubagent,
        Self::CodexRollout,
        Self::OpenCodeDatabase,
        Self::KimiState,
        Self::KimiWire,
        Self::GrokSummary,
        Self::GrokUpdates,
    ];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub profile: PathBuf,
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub modified: i64,
    pub bytes: Vec<u8>,
}

impl Artifact {
    pub fn locator(&self) -> ArtifactLocator {
        ArtifactLocator {
            profile: self.profile.clone(),
            path: self.path.clone(),
            kind: self.kind,
            modified: self.modified,
            bytes: self.bytes.len() as u64,
        }
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn head_tail(&self, head: usize, tail: usize) -> (String, String) {
        let start = self.bytes.len().min(head);
        let tail_start = self.bytes.len().saturating_sub(tail);
        (
            String::from_utf8_lossy(&self.bytes[..start]).into_owned(),
            String::from_utf8_lossy(&self.bytes[tail_start..]).into_owned(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactLocator {
    pub profile: PathBuf,
    pub path: PathBuf,
    pub kind: ArtifactKind,
    pub modified: i64,
    pub bytes: u64,
}

pub fn artifact(
    profile: impl Into<PathBuf>,
    path: impl Into<PathBuf>,
    kind: ArtifactKind,
    modified: i64,
    bytes: impl Into<Vec<u8>>,
) -> Artifact {
    Artifact {
        profile: profile.into(),
        path: path.into(),
        kind,
        modified,
        bytes: bytes.into(),
    }
}

pub fn engine_for_kind(kind: ArtifactKind) -> Option<Engine> {
    match kind {
        ArtifactKind::ClaudeMain | ArtifactKind::ClaudeSubagent => Some(Engine::Claude),
        ArtifactKind::CodexRollout => Some(Engine::Codex),
        ArtifactKind::OpenCodeDatabase => Some(Engine::Opencode),
        ArtifactKind::KimiState | ArtifactKind::KimiWire => Some(Engine::Kimi),
        ArtifactKind::GrokSummary | ArtifactKind::GrokUpdates => Some(Engine::Grok),
    }
}
