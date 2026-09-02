use std::sync::{Arc, Mutex};

use super::super::{
    LaunchOptions, LaunchPlan, LaunchResult, NoopLauncher, PlatformLauncher, PreservedEnvironment,
    build_launch_plan, run_launch,
};
use crate::Engine;
use crate::Result;

#[derive(Clone, Default)]
struct Recorder {
    plans: Arc<Mutex<Vec<String>>>,
}

impl PlatformLauncher for Recorder {
    fn launch(&self, plan: &LaunchPlan) -> Result<LaunchResult> {
        self.plans.lock().unwrap().push(plan.shell_command.clone());
        Ok(LaunchResult::Launched)
    }
}

fn plan() -> LaunchPlan {
    build_launch_plan(&LaunchOptions {
        engine: Engine::Grok,
        source_account: "1".into(),
        target_account: "2".into(),
        reason: "test".into(),
        cwd: "/workspace".into(),
        kickoff: "resume".into(),
        worker_scope: None,
        model_overrides: Default::default(),
        environment: PreservedEnvironment::default(),
        headless: true,
        session_id: None,
        resume: false,
    })
    .unwrap()
}

#[test]
fn dry_run_has_exactly_zero_platform_side_effects() {
    let recorder = Recorder::default();
    assert_eq!(
        run_launch(&recorder, &plan(), true).unwrap(),
        LaunchResult::DryRun
    );
    assert!(recorder.plans.lock().unwrap().is_empty());
}

#[test]
fn platform_trait_receives_the_exact_rendered_command() {
    let recorder = Recorder::default();
    assert_eq!(
        run_launch(&recorder, &plan(), false).unwrap(),
        LaunchResult::Launched
    );
    let expected = if cfg!(windows) {
        "Set-Location -LiteralPath '/workspace'; & 'gmax' '2' 'resume'"
    } else {
        "cd '/workspace' && gmax '2' 'resume'"
    };
    assert_eq!(recorder.plans.lock().unwrap().as_slice(), [expected]);
}

#[test]
fn noop_launcher_does_not_try_to_open_terminal() {
    assert_eq!(
        run_launch(&NoopLauncher, &plan(), false).unwrap(),
        LaunchResult::NotLaunched
    );
}
