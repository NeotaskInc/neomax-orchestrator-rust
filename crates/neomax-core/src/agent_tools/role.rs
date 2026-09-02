use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchRole {
    Orchestrator,
    #[default]
    Worker,
}

impl LaunchRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orchestrator => "orchestrator",
            Self::Worker => "worker",
        }
    }

    pub const fn policy_name(self) -> &'static str {
        self.as_str()
    }

    pub const fn is_orchestrator(self) -> bool {
        matches!(self, Self::Orchestrator)
    }
}
