use crate::config::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionRejection {
    Fleet {
        active: u32,
        maximum: u32,
    },
    Task {
        active: u32,
        maximum: u32,
    },
    Provider {
        engine: Engine,
        active: u32,
        maximum: u32,
    },
    AccountLanes {
        account: String,
        active: u32,
        maximum: u32,
    },
    AccountSessions {
        account: String,
        active: u32,
        maximum: u32,
    },
}

impl std::fmt::Display for AdmissionRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fleet { active, maximum } => {
                write!(formatter, "fleet dispatch cap reached: {active}/{maximum}")
            }
            Self::Task { active, maximum } => {
                write!(formatter, "task dispatch cap reached: {active}/{maximum}")
            }
            Self::Provider {
                engine,
                active,
                maximum,
            } => write!(
                formatter,
                "provider {engine} dispatch cap reached: {active}/{maximum}"
            ),
            Self::AccountLanes {
                account,
                active,
                maximum,
            } => write!(
                formatter,
                "account {account} lane cap reached: {active}/{maximum}"
            ),
            Self::AccountSessions {
                account,
                active,
                maximum,
            } => write!(
                formatter,
                "account {account} session cap reached: {active}/{maximum}"
            ),
        }
    }
}

impl AdmissionRejection {
    pub fn active_maximum(&self) -> (u32, u32) {
        match self {
            Self::Fleet { active, maximum }
            | Self::Task { active, maximum }
            | Self::Provider {
                active, maximum, ..
            }
            | Self::AccountLanes {
                active, maximum, ..
            }
            | Self::AccountSessions {
                active, maximum, ..
            } => (*active, *maximum),
        }
    }
}
