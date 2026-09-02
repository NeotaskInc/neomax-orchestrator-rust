use crate::config::Engine;
use crate::{Error, Result};

use super::constants::MAX_ID_LEN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    pub lease_id: String,
    pub task_id: String,
    pub engine: Option<Engine>,
}

impl AdmissionRequest {
    pub fn new(
        lease_id: impl Into<String>,
        task_id: impl Into<String>,
        engine: Option<Engine>,
    ) -> Self {
        Self {
            lease_id: lease_id.into(),
            task_id: task_id.into(),
            engine,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for (name, value) in [("lease id", &self.lease_id), ("task id", &self.task_id)] {
            if value.is_empty()
                || value.len() > MAX_ID_LEN
                || value.starts_with('.')
                || value.ends_with('.')
                || value.contains("..")
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
            {
                return Err(Error::InvalidArgument(format!(
                    "dispatch admission {name} is invalid"
                )));
            }
        }
        Ok(())
    }
}
