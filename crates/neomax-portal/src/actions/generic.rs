use std::collections::BTreeMap;

use anyhow::Result;

use super::planner::{ActionContext, ActionKind, ActionPlan};
use super::validation::{parse_engine, validate_account, validate_run_id};

pub(crate) fn plan_pause(
    context: &ActionContext,
    raw_engine: &str,
    raw_account: &str,
    paused: bool,
) -> Result<ActionPlan> {
    context.validate_roots()?;
    let engine = parse_engine(raw_engine)?;
    let account = validate_account(raw_account)?;
    let program = context.validated_neomax_binary()?;
    Ok(ActionPlan {
        operation: if paused { "pause" } else { "unpause" }.into(),
        program,
        args: vec![
            if paused { "pause" } else { "unpause" }.into(),
            account.clone(),
            "--engine".into(),
            engine.as_str().into(),
        ],
        environment: neomax_environment(context),
        engine: Some(engine.as_str().into()),
        account: Some(account),
        run_id: None,
        destructive: false,
        confirmation_required: false,
        message: if paused {
            "account paused and excluded from automatic dispatch".into()
        } else {
            "account unpaused and eligible for automatic dispatch".into()
        },
    })
}

pub(crate) fn plan_run(
    context: &ActionContext,
    action: ActionKind,
    raw_run_id: &str,
) -> Result<ActionPlan> {
    context.validate_roots()?;
    let run_id = validate_run_id(raw_run_id)?;
    let program = context.validated_neomax_binary()?;
    Ok(ActionPlan {
        operation: action.as_str().into(),
        program,
        args: vec![action.as_str().into(), run_id.clone()],
        environment: neomax_environment(context),
        engine: None,
        account: None,
        run_id: Some(run_id),
        destructive: action.destructive(),
        confirmation_required: action.destructive(),
        message: format!("{} requested for the selected run", action.as_str()),
    })
}

fn neomax_environment(context: &ActionContext) -> BTreeMap<String, String> {
    BTreeMap::from([(
        "NEOMAX_HOME".into(),
        context.state.to_string_lossy().into_owned(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> (tempfile::TempDir, ActionContext) {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let state = temp.path().join("state");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let mut context = ActionContext::from_home(home, state);
        context.neomax_binary = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        (temp, context)
    }

    #[test]
    fn destructive_actions_require_confirmation_and_safe_run_ids() {
        let (_temp, context) = context();
        let plan = plan_run(&context, ActionKind::Kill, "20260823-120000-123").unwrap();
        assert!(plan.destructive);
        assert!(plan.confirmation_required);
        assert!(plan_run(&context, ActionKind::Kill, "../../secret").is_err());
    }

    #[test]
    fn pause_plans_use_the_selected_engine_and_state_home() {
        let (_temp, context) = context();
        let expected_state = context.state.to_string_lossy().into_owned();
        let plan = plan_pause(&context, "grok", "3", true).unwrap();
        assert_eq!(plan.args, ["pause", "3", "--engine", "grok"]);
        assert_eq!(plan.environment.get("NEOMAX_HOME"), Some(&expected_state));
    }
}
