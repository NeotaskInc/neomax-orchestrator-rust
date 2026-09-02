use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_SUPERVISOR_LEASE_SECONDS: i64 = 30;

pub(crate) fn initial_revision() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorLease {
    pub owner: String,
    #[serde(default)]
    pub pid: Option<u32>,
    pub acquired_at: i64,
    pub heartbeat_at: i64,
    pub expires_at: i64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl SupervisorLease {
    pub fn new(
        owner: impl Into<String>,
        pid: Option<u32>,
        now: i64,
        ttl_seconds: i64,
    ) -> crate::Result<Self> {
        let owner = owner.into();
        if owner.trim().is_empty() {
            return Err(crate::Error::InvalidArgument(
                "scheduler supervisor owner cannot be empty".into(),
            ));
        }
        if ttl_seconds <= 0 {
            return Err(crate::Error::InvalidArgument(
                "scheduler supervisor lease TTL must be positive".into(),
            ));
        }
        let expires_at = now.checked_add(ttl_seconds).ok_or_else(|| {
            crate::Error::InvalidArgument("scheduler supervisor lease expiry overflowed".into())
        })?;
        Ok(Self {
            owner,
            pid,
            acquired_at: now,
            heartbeat_at: now,
            expires_at,
            extra: BTreeMap::new(),
        })
    }

    pub fn is_live(&self, now: i64) -> bool {
        now < self.expires_at
    }

    pub fn heartbeat(&mut self, now: i64, ttl_seconds: i64) -> crate::Result<()> {
        if ttl_seconds <= 0 {
            return Err(crate::Error::InvalidArgument(
                "scheduler supervisor lease TTL must be positive".into(),
            ));
        }
        let expires_at = now.checked_add(ttl_seconds).ok_or_else(|| {
            crate::Error::InvalidArgument("scheduler supervisor lease expiry overflowed".into())
        })?;
        self.heartbeat_at = now;
        self.expires_at = expires_at;
        Ok(())
    }

    pub fn validate(&self, created_at: i64) -> crate::Result<()> {
        if self.owner.trim().is_empty() {
            return Err(crate::Error::InvalidState {
                path: "scheduler".into(),
                message: "scheduler supervisor lease owner is empty".into(),
            });
        }
        if self.acquired_at < created_at
            || self.heartbeat_at < self.acquired_at
            || self.expires_at <= self.heartbeat_at
        {
            return Err(crate::Error::InvalidState {
                path: "scheduler".into(),
                message: "scheduler supervisor lease timestamps are out of order".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    Running,
    Done,
    Failed,
    Interrupted,
    Killed,
    #[serde(other)]
    Unknown,
}

impl PlanStatus {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Interrupted | Self::Killed | Self::Unknown
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanControlMarkers {
    pub killed: bool,
    pub interrupted: bool,
    pub kill_requested: bool,
}
