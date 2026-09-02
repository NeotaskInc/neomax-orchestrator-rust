use std::process::Stdio;

use anyhow::Result;
use neomax_core::providers::{is_secret_environment_key, scrub_provider_environment};
use neomax_core::runtime;

use super::planner::ActionPlan;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ActionExecution {
    pub executed: bool,
    #[serde(default)]
    pub pid: Option<u32>,
    pub message: String,
}

pub trait LocalActionExecutor: Send + Sync {
    fn execute(&self, plan: &ActionPlan) -> Result<ActionExecution>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemActionExecutor;

impl LocalActionExecutor for SystemActionExecutor {
    fn execute(&self, plan: &ActionPlan) -> Result<ActionExecution> {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let mut command = runtime::process_command(&plan.program, &plan.args, &current_dir)?;
        scrub_provider_environment(&mut command);
        for (key, value) in &plan.environment {
            if !is_secret_environment_key(key) {
                command.env(key, value);
            }
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn()?;
        Ok(ActionExecution {
            executed: true,
            pid: Some(child.id()),
            message: plan.message.clone(),
        })
    }
}
