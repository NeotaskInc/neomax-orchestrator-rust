use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandClass {
    ReadOnly,
    Mutating,
    Destructive,
    External,
}

impl CommandClass {
    pub const ALL: [Self; 4] = [
        Self::ReadOnly,
        Self::Mutating,
        Self::Destructive,
        Self::External,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Mutating => "mutating",
            Self::Destructive => "destructive",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandFamily {
    Accounts,
    Configuration,
    Dispatch,
    Git,
    Help,
    Issues,
    Lifecycle,
    Orchestration,
    Projects,
    Queue,
    Sessions,
    Tasks,
    Usage,
    Workers,
}

impl CommandFamily {
    pub const ALL: [Self; 14] = [
        Self::Accounts,
        Self::Configuration,
        Self::Dispatch,
        Self::Git,
        Self::Help,
        Self::Issues,
        Self::Lifecycle,
        Self::Orchestration,
        Self::Projects,
        Self::Queue,
        Self::Sessions,
        Self::Tasks,
        Self::Usage,
        Self::Workers,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accounts => "accounts",
            Self::Configuration => "configuration",
            Self::Dispatch => "dispatch",
            Self::Git => "git",
            Self::Help => "help",
            Self::Issues => "issues",
            Self::Lifecycle => "lifecycle",
            Self::Orchestration => "orchestration",
            Self::Projects => "projects",
            Self::Queue => "queue",
            Self::Sessions => "sessions",
            Self::Tasks => "tasks",
            Self::Usage => "usage",
            Self::Workers => "workers",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalCommand {
    pub family: CommandFamily,
    pub command: &'static str,
    pub class: CommandClass,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrchestratorHost {
    Claude,
    Codex,
    Opencode,
    Kimi,
    Grok,
}

impl OrchestratorHost {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Opencode,
        Self::Kimi,
        Self::Grok,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Kimi => "kimi",
            Self::Grok => "grok",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestCommand {
    pub family: CommandFamily,
    pub command: String,
    pub class: CommandClass,
    pub summary: String,
}

impl ManifestCommand {
    pub(crate) fn from_canonical(command: CanonicalCommand) -> Self {
        Self {
            family: command.family,
            command: command.command.into(),
            class: command.class,
            summary: command.summary.into(),
        }
    }
}
