use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};

use anyhow::{Result, bail};
use neomax_core::agent_tools::ORCHESTRATOR_TOOL_INSTRUCTION;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::projects::ProjectOrientation;
use neomax_core::providers::catalog;
use neomax_core::{Engine, WorkerScope};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::output;
use crate::parser;

pub(super) fn run(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let environment = env::vars_os().collect::<BTreeMap<_, _>>();
    run_with_environment(launcher, args, context, &environment)
}

fn run_with_environment(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<()> {
    let hook = parser::has(args, "--hook");
    let json_output = parser::has(args, "--json");
    for arg in args {
        if !matches!(arg.as_str(), "--hook" | "--json") {
            bail!("orient: unknown option {arg}");
        }
    }
    if hook {
        if !interactive_orchestrator(environment) {
            return Ok(());
        }
        let directive = directive(launcher, context)?;
        return output::json(&json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": directive,
            }
        }));
    }
    if !interactive_orchestrator(environment) {
        bail!("neomax orient is available only inside an orchestrator session")
    }
    let directive = directive(launcher, context)?;
    if json_output {
        return output::json(&json!({"directive": directive}));
    }
    println!("{directive}");
    Ok(())
}

fn interactive_orchestrator(environment: &BTreeMap<OsString, OsString>) -> bool {
    environment.contains_key(OsStr::new("NEOMAX_ROLE"))
        && !environment.contains_key(OsStr::new("NEOMAX_WORKER"))
        && environment
            .get(OsStr::new("NEOMAX_MODE"))
            .map(OsString::as_os_str)
            != Some(OsStr::new("solo"))
}

fn directive(launcher: Launcher, context: &RuntimeContext) -> Result<String> {
    directive_with_facts(launcher, None, None, None, context)
}

fn directive_with_facts(
    launcher: Launcher,
    engine_override: Option<Engine>,
    scope_override: Option<&WorkerScope>,
    worker_models_override: Option<&BTreeMap<Engine, String>>,
    context: &RuntimeContext,
) -> Result<String> {
    let engine = engine_override
        .or_else(|| launcher_engine(launcher))
        .or_else(|| env::var("NEOMAX_ENGINE").ok()?.parse().ok())
        .unwrap_or(Engine::Claude);
    let scope = scope_override.cloned().unwrap_or_else(|| {
        env::var("NEOMAX_FLEET")
            .or_else(|_| env::var("NEOMAX_WORKERS"))
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(WorkerScope::all)
    });
    let environment = env::vars().collect::<BTreeMap<_, _>>();
    let overrides = context.model_overrides()?;
    let model = match worker_models_override.and_then(|models| models.get(&engine).cloned()) {
        Some(model) => model,
        None => selected_model(engine, &overrides, &environment)?,
    };
    let project_orientation = context.project_registry().orientation_of(&context.cwd);
    let project = project_orientation
        .as_ref()
        .map_or_else(|| "current project".into(), |project| project.name.clone());
    let workers = scope.csv();
    let worker_models = Engine::ALL
        .into_iter()
        .filter(|engine| scope.contains(*engine))
        .map(|engine| {
            let model = worker_models_override
                .and_then(|models| models.get(&engine).cloned())
                .or_else(|| selected_model(engine, &overrides, &environment).ok())
                .unwrap_or_else(|| catalog::default_model_id(engine).into());
            format!("{}={model}", engine.as_str())
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let project_facts = render_project_facts(project_orientation.as_ref());
    Ok(format!(
        "SYSTEM - Neomax orchestrator session\n\nOrchestrator: {engine} {model}\nWorker scope: {workers}\nWorker model defaults: {worker_models}\nProject: {project}\nWorking directory: {}\nConcurrency: max-subagents={} max-tasks={} lanes-per-account={}\n\n{}\n\nUse the complete Neomax toolbox: delegate work, run plans, inspect status and usage, recover runs, rotate eligible accounts, and use provider-supported models. Keep work scoped to the current project and preserve the durable run record.\n\nOperating principle: use sub-agents and workers, run parallel work concurrently, and spread work evenly across available accounts.\n\nStart by reading the project AGENTS.md and CLAUDE.md files, then run `neomax status` before dispatching work.{}",
        context.cwd.display(),
        context.settings.concurrency.max_subagents,
        context.settings.concurrency.max_tasks,
        context.settings.concurrency.lanes_per_account,
        ORCHESTRATOR_TOOL_INSTRUCTION,
        project_facts,
    ))
}

pub(crate) fn no_task_instruction(
    launcher: Launcher,
    engine: Engine,
    scope: &WorkerScope,
    worker_models: &BTreeMap<Engine, String>,
    context: &RuntimeContext,
) -> Result<String> {
    directive_with_facts(
        launcher,
        Some(engine),
        Some(scope),
        Some(worker_models),
        context,
    )
}

fn render_project_facts(project: Option<&ProjectOrientation>) -> String {
    let Some(project) = project else {
        return "Project orientation: no registered project owns the current directory. Use `neomax projects` to inspect available projects before changing focus.".into();
    };
    let locations = [
        ("brain", project.relative_location(project.brain.as_deref())),
        (
            "agents",
            project.relative_location(project.agents.as_deref()),
        ),
        (
            "orchestrator brain",
            project.relative_location(project.orch_brain.as_deref()),
        ),
        (
            "planning home",
            project.relative_location(project.planning.as_deref()),
        ),
    ]
    .into_iter()
    .filter_map(|(label, path)| path.map(|path| format!("{label}={path}")))
    .collect::<Vec<_>>();
    let repos = if project.repos.is_empty() {
        "(none registered)".into()
    } else {
        project
            .repos
            .iter()
            .map(|repo| repo.to_string_lossy())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut facts = format!(
        "Project orientation: registered project `{}`; repositories: {}; branch prefix: {}.\nProject locations: {}.",
        project.name,
        repos,
        project.branch_prefix.as_deref().unwrap_or("(none)"),
        if locations.is_empty() {
            "(no configured locations)".into()
        } else {
            locations.join("; ")
        },
    );
    if let Some(content) = project.opener_content.as_deref() {
        let bounded = content.chars().take(4096).collect::<String>();
        facts.push_str("\nProject opener supplement (product-safe, bounded):\n");
        facts.push_str(&bounded);
        if bounded.chars().count() < content.chars().count() {
            facts.push_str("\n[opener content truncated]");
        }
    }
    facts
}

fn selected_model(
    engine: Engine,
    overrides: &crate::models::ModelOverrides,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    Ok(overrides
        .effective_model_with_environment(engine, None, environment)?
        .model)
}

fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn manual_orientation_requires_an_orchestrator_role() {
        let fixture = fixture();
        let environment = BTreeMap::new();
        assert!(
            run_with_environment(Launcher::Universal, &[], &fixture.context, &environment).is_err()
        );
    }

    #[test]
    fn directive_uses_pinned_engine_and_scope_without_personal_data() {
        let fixture = fixture();
        let text = directive(
            Launcher::ProviderOrchestrator(Engine::Opencode),
            &fixture.context,
        )
        .unwrap();
        assert!(text.contains("Orchestrator: opencode"));
        assert!(text.contains("Worker scope: claude,codex,opencode,kimi,grok"));
        let private_name = ["da", "bull", "ock"].concat();
        assert!(!text.contains(private_name.as_str()));
    }

    #[test]
    fn registered_project_orientation_adds_safe_locations_and_opener() {
        let fixture = fixture();
        let opener = fixture.context.cwd.join("docs/opener.md");
        std::fs::create_dir_all(opener.parent().unwrap()).unwrap();
        std::fs::write(&opener, "Use the project rules.\nVerify the result.\n").unwrap();
        let project = neomax_core::projects::Project {
            opener: Some("docs/opener.md".into()),
            ..neomax_core::projects::Project::portable(
                fixture.context.cwd.clone(),
                "fix".into(),
                fixture.context.now,
            )
        };
        fixture
            .context
            .project_registry()
            .register("fixture", project, false)
            .unwrap();
        let text = directive(
            Launcher::ProviderOrchestrator(Engine::Grok),
            &fixture.context,
        )
        .unwrap();
        assert!(text.contains("Project: fixture"));
        assert!(text.contains("brain=CLAUDE.md"));
        assert!(text.contains("agents=AGENTS.md"));
        assert!(text.contains("orchestrator brain=docs/neomax-orchestrator/ORCHESTRATOR.md"));
        assert!(text.contains("planning home=docs/neomax-orchestrator"));
        assert!(text.contains("Use the project rules."));
        assert!(text.contains("Worker model defaults:"));
        assert!(text.contains("Concurrency: max-subagents="));
    }

    #[test]
    fn project_opener_content_is_not_loaded_from_an_unregistered_path() {
        let fixture = fixture();
        let outside = fixture.context.cwd.parent().unwrap().join("outside.md");
        std::fs::write(&outside, "outside content").unwrap();
        let project = neomax_core::projects::Project {
            opener: Some("../outside.md".into()),
            ..neomax_core::projects::Project::portable(
                fixture.context.cwd.clone(),
                "fix".into(),
                fixture.context.now,
            )
        };
        fixture
            .context
            .project_registry()
            .register("fixture", project, false)
            .unwrap();
        let text = directive(
            Launcher::ProviderOrchestrator(Engine::Codex),
            &fixture.context,
        )
        .unwrap();
        assert!(!text.contains("outside content"));
        assert!(!text.contains("../outside.md"));
    }

    #[test]
    fn model_selection_uses_config_before_environment_and_default() {
        let environment = BTreeMap::from([(
            "NEOMAX_OPENCODE_MODEL".into(),
            "environment/opencode".into(),
        )]);
        let configured = crate::models::ModelOverrides {
            opencode: Some("config/opencode".into()),
            ..crate::models::ModelOverrides::default()
        };
        assert_eq!(
            selected_model(Engine::Opencode, &configured, &environment).unwrap(),
            "config/opencode"
        );
        assert_eq!(
            selected_model(
                Engine::Opencode,
                &crate::models::ModelOverrides::default(),
                &environment,
            )
            .unwrap(),
            "environment/opencode"
        );
        assert_eq!(
            selected_model(
                Engine::Kimi,
                &crate::models::ModelOverrides::default(),
                &BTreeMap::new(),
            )
            .unwrap(),
            "kimi-code/k3"
        );
    }

    #[test]
    fn launch_orientation_uses_resolved_scope_and_models_not_process_defaults() {
        let fixture = fixture();
        let scope = "grok".parse::<WorkerScope>().unwrap();
        let models = BTreeMap::from([(Engine::Grok, "xai/local-grok".into())]);
        let text = no_task_instruction(
            Launcher::Universal,
            Engine::Grok,
            &scope,
            &models,
            &fixture.context,
        )
        .unwrap();
        assert!(text.contains("Orchestrator: grok xai/local-grok"));
        assert!(text.contains("Worker scope: grok"));
        assert!(text.contains("Worker model defaults: grok=xai/local-grok"));
        assert!(!text.contains("Worker scope: claude,codex,opencode,kimi,grok"));
    }
}
