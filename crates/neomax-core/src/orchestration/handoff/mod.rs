mod advice;
mod baton;
mod command;
mod platform;
mod role;
mod selection;

#[cfg(test)]
mod tests;

pub use advice::{HandoffAdvice, HandoffCheck, check_result, rotation_advice};
pub use baton::{AccountId, HandoffBaton, HandoffStore};
pub use command::{
    LaunchOptions, LaunchPlan, PreservedEnvironment, ShellKind, build_launch_plan, default_kickoff,
    launcher_for, render_shell_command, render_shell_command_for, shell_quote,
};
pub use platform::{LaunchResult, NoopLauncher, PlatformLauncher, run_launch};
pub use role::{
    OrchestratorIdentity, config_env, current_profile, identity, infer_engine, profile_for_engine,
};
pub use selection::{
    HandoffTargetRequest, TargetEligibility, TargetPolicy, TargetSelection, TargetTier,
    eligibility, parse_account_selectors, select_reserved_orchestrator, select_target,
};
