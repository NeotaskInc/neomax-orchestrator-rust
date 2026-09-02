use crate::Result;

use super::command::LaunchPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchResult {
    DryRun,
    Launched,
    NotLaunched,
}

pub trait PlatformLauncher {
    fn launch(&self, plan: &LaunchPlan) -> Result<LaunchResult>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopLauncher;

impl PlatformLauncher for NoopLauncher {
    fn launch(&self, _plan: &LaunchPlan) -> Result<LaunchResult> {
        Ok(LaunchResult::NotLaunched)
    }
}

pub fn run_launch<L: PlatformLauncher>(
    launcher: &L,
    plan: &LaunchPlan,
    dry_run: bool,
) -> Result<LaunchResult> {
    if dry_run {
        return Ok(LaunchResult::DryRun);
    }
    launcher.launch(plan)
}
