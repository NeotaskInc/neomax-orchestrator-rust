use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use neomax_core::config::Engine;
use neomax_core::providers::catalog;

use super::planner::{ActionContext, ActionPlan};
use super::validation::{parse_engine, validate_account};

pub(crate) fn plan_connect(
    context: &ActionContext,
    raw_engine: &str,
    raw_account: &str,
) -> Result<ActionPlan> {
    context.validate_roots()?;
    let engine = parse_engine(raw_engine)?;
    let account = validate_account(raw_account)?;
    let account_arg = if account == "orch" {
        "orch".to_string()
    } else {
        account.clone()
    };
    let profile = profile_path(context, engine, &account);
    let (program, args, environment, message) = match engine {
        Engine::Claude => (
            "cmax".into(),
            vec![if account == "orch" {
                "orchestrator".into()
            } else {
                account_arg
            }],
            BTreeMap::new(),
            "Claude account helper started; complete /login in that session".into(),
        ),
        Engine::Codex => (
            "codex".into(),
            vec!["login".into()],
            BTreeMap::from([("CODEX_HOME".into(), profile.to_string_lossy().into_owned())]),
            "Codex login started for the selected account".into(),
        ),
        Engine::Opencode => (
            "ocx".into(),
            vec!["login".into(), account_arg],
            BTreeMap::new(),
            "OpenCode login started for the selected account".into(),
        ),
        Engine::Kimi => (
            "kmx".into(),
            vec!["login".into(), account_arg, "choose".into()],
            BTreeMap::new(),
            "Kimi login started for the selected account".into(),
        ),
        Engine::Grok => (
            "gmx".into(),
            vec!["login".into(), account_arg, "choose".into()],
            BTreeMap::new(),
            "Grok login started for the selected account".into(),
        ),
    };
    Ok(ActionPlan {
        operation: "connect".into(),
        program,
        args,
        environment,
        engine: Some(engine.as_str().into()),
        account: Some(account),
        run_id: None,
        destructive: false,
        confirmation_required: false,
        message,
    })
}

fn profile_path(context: &ActionContext, engine: Engine, account: &str) -> PathBuf {
    let spec = catalog::spec(engine);
    if account == "orch" {
        context.home.join(spec.orchestrator_dir)
    } else if account == "1" {
        context.home.join(spec.default_profile_dir)
    } else {
        context
            .home
            .join(format!("{}{}", spec.account_prefix, account))
    }
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
        (temp, ActionContext::from_home(home, state))
    }

    #[test]
    fn provider_connect_plans_are_argument_arrays_without_shell_interpolation() {
        let (_temp, context) = context();
        let plan = plan_connect(&context, "kimi", "2").unwrap();
        assert_eq!(plan.program, "kmx");
        assert_eq!(plan.args, ["login", "2", "choose"]);
        assert!(plan.environment.is_empty());
    }

    #[test]
    fn every_provider_has_a_deterministic_connect_plan() {
        for (engine, program) in [
            (Engine::Claude, "cmax"),
            (Engine::Codex, "codex"),
            (Engine::Opencode, "ocx"),
            (Engine::Kimi, "kmx"),
            (Engine::Grok, "gmx"),
        ] {
            let (_temp, context) = context();
            let plan = plan_connect(&context, engine.as_str(), "2").unwrap();
            assert_eq!(plan.program, program);
            assert_eq!(plan.engine.as_deref(), Some(engine.as_str()));
            assert_eq!(plan.account.as_deref(), Some("2"));
        }
    }
}
