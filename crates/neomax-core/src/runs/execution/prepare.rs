use std::collections::BTreeMap;

#[cfg(test)]
use std::path::PathBuf;

use crate::providers::catalog;
use crate::providers::kimi_plan::PreparedHome;
use crate::providers::orchestrator::ORCHESTRATOR_ORIENTATION_ENV;
use crate::providers::{
    OrchestratorEnvironment, OrchestratorRequest, Provider, ProviderCommand, ProviderProcessSecret,
    ProviderProfile, WorkerLaunchContext, WorkerRequest, kimi_agent_file, kimi_plan,
};
use crate::runs::RunRecord;
use crate::{EffectiveSettings, Error, Result, StatePaths, WorkerScope};

use super::tooling::{WorkerToolingInput, prepare_worker_tools};

pub struct PreparedAttempt {
    command: ProviderCommand,
    _launch_context: WorkerLaunchContext,
    _kimi_plan_home: Option<PreparedHome>,
    orchestrator_request: Option<OrchestratorRequest>,
    bootstrap_command: Option<ProviderCommand>,
}

impl PreparedAttempt {
    fn new(
        command: ProviderCommand,
        launch_context: WorkerLaunchContext,
        kimi_plan_home: Option<PreparedHome>,
        orchestrator_request: Option<OrchestratorRequest>,
        bootstrap_command: Option<ProviderCommand>,
    ) -> Self {
        Self {
            command,
            _launch_context: launch_context,
            _kimi_plan_home: kimi_plan_home,
            orchestrator_request,
            bootstrap_command,
        }
    }

    #[cfg(test)]
    pub fn from_command(command: ProviderCommand) -> Self {
        let profile = ProviderProfile {
            engine: crate::Engine::Claude,
            account: "fixture".into(),
            path: PathBuf::from("/fixture/profile"),
            reserved: false,
        };
        let request = WorkerRequest::new(profile, command.cwd.clone(), "fixture");
        Self {
            command,
            _launch_context: WorkerLaunchContext::for_test(request),
            _kimi_plan_home: None,
            orchestrator_request: None,
            bootstrap_command: None,
        }
    }

    #[cfg(test)]
    pub fn with_bootstrap(
        mut self,
        bootstrap_command: ProviderCommand,
        orchestrator_request: OrchestratorRequest,
    ) -> Self {
        self.bootstrap_command = Some(bootstrap_command);
        self.orchestrator_request = Some(orchestrator_request);
        self
    }

    pub fn command(&self) -> &ProviderCommand {
        &self.command
    }

    pub fn bootstrap_command(&self) -> Option<&ProviderCommand> {
        self.bootstrap_command.as_ref()
    }

    pub fn resumed_orchestrator_command(
        &self,
        provider: &dyn Provider,
        session: &str,
    ) -> Result<ProviderCommand> {
        let Some(request) = self.orchestrator_request.as_ref() else {
            return Err(Error::InvalidArgument(
                "a resumed orchestrator command requires an orchestrator request".into(),
            ));
        };
        let mut request = request.clone();
        request.environment.session = session.to_owned();
        request.initial_task = None;
        request.session_id = Some(session.to_owned());
        request.resume = true;
        provider.orchestrator_command(&request)
    }
}

pub fn prepare_attempt(
    provider: &dyn Provider,
    run: &RunRecord,
    settings: &EffectiveSettings,
    paths: &StatePaths,
    resume_session: Option<&str>,
) -> Result<PreparedAttempt> {
    prepare_attempt_with_secret(provider, run, settings, paths, resume_session, None)
}

pub fn prepare_attempt_with_secret(
    provider: &dyn Provider,
    run: &RunRecord,
    settings: &EffectiveSettings,
    paths: &StatePaths,
    resume_session: Option<&str>,
    process_secret: Option<ProviderProcessSecret>,
) -> Result<PreparedAttempt> {
    if provider.engine() != run.engine {
        return Err(Error::Conflict(format!(
            "run {} targets {} but provider adapter targets {}",
            run.id,
            run.engine,
            provider.engine()
        )));
    }
    let profile = ProviderProfile {
        engine: run.engine,
        account: run.account(),
        path: run.profile.clone(),
        reserved: run
            .extra
            .get("orchestrator_reserved")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| run.account().eq_ignore_ascii_case("orch")),
    };
    let mut request = WorkerRequest::new(profile, &run.workdir, run.prompt_for_attempt());
    request.model = (!run.model.is_empty()).then(|| run.model.clone());
    request.goal = run.goal.clone();
    request.session_id = run.session.clone();
    request.resume_session = (run.launch_role.is_orchestrator()
        || catalog::supports_native_resume(run.engine))
        .then(|| resume_session.map(str::to_string))
        .flatten();
    request.effort = run.effort.clone();
    request.max_turns = run.max_turns;
    request.plan = run.plan_mode;
    request.ultra = run.ultra;
    request = request.with_process_secret(process_secret);
    request.agent_environment = settings.agent_environment();
    request.agent_environment.extend(
        run.environment
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    request.set_launch_role(run.launch_role);

    let kimi_plan_home = if run.engine == crate::Engine::Kimi && run.plan_mode {
        let home = kimi_plan::prepare(&run.profile, &paths.state)?;
        request.config_home_override = Some(home.path().to_path_buf());
        Some(home)
    } else {
        None
    };
    let tools = prepare_worker_tools(WorkerToolingInput::from_runtime(paths, settings, &request))?;
    let launch_context = WorkerLaunchContext::new(request, tools);
    let (command, orchestrator_request, bootstrap_command) = if run.launch_role.is_orchestrator() {
        let worker_models = worker_models(settings, run)?;
        let request = orchestrator_request(paths, run, &launch_context, &worker_models)?;
        let bootstrap = provider.orchestrator_bootstrap_command(&request)?;
        (
            provider.orchestrator_command(&request)?,
            Some(request),
            bootstrap,
        )
    } else {
        (provider.worker_command(&launch_context)?, None, None)
    };
    Ok(PreparedAttempt::new(
        command,
        launch_context,
        kimi_plan_home,
        orchestrator_request,
        bootstrap_command,
    ))
}

fn orchestrator_request(
    paths: &StatePaths,
    run: &RunRecord,
    context: &WorkerLaunchContext,
    worker_models: &BTreeMap<crate::Engine, String>,
) -> Result<OrchestratorRequest> {
    let scope = run
        .extra
        .get("worker_scope")
        .and_then(serde_json::Value::as_str)
        .map(str::parse::<WorkerScope>)
        .transpose()?
        .unwrap_or_else(WorkerScope::all);
    let mut environment =
        OrchestratorEnvironment::new(scope, run.session.clone().unwrap_or_else(|| run.id.clone()))
            .reserved(context.request().profile.reserved);
    if let Some(pid) = run.supervisor_pid {
        environment = environment.with_pid(pid);
    }
    for (engine, model) in worker_models {
        environment = environment.with_model(*engine, model.clone());
    }
    for (key, value) in &context.request().agent_environment {
        environment = environment.with_variable(key.clone(), value.clone());
    }
    let request = context.request();
    let mut orchestrator = OrchestratorRequest::new(
        ProviderProfile {
            engine: request.profile.engine,
            account: request.profile.account.clone(),
            path: request.profile.path.clone(),
            reserved: request.profile.reserved,
        },
        &paths.home,
        &request.cwd,
        environment,
    )
    .with_model(request.selected_model())
    .with_process_secret(request.process_secret.clone())
    .with_ultra(request.ultra)
    .with_solo(
        run.extra
            .get("solo")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    );
    if let Some(orientation) = context
        .request()
        .agent_environment
        .get(ORCHESTRATOR_ORIENTATION_ENV)
    {
        orchestrator = orchestrator.with_orientation(orientation.clone());
    }
    if let Some(effort) = request.effort.as_deref() {
        orchestrator = orchestrator.with_effort(effort);
    }
    if let Some(goal) = request.goal.as_deref() {
        orchestrator = orchestrator.with_goal(goal);
    }
    if let Some(max_turns) = request.max_turns {
        orchestrator = orchestrator.with_max_turns(max_turns);
    }
    if request.profile.engine == crate::Engine::Kimi {
        orchestrator = orchestrator.with_agent_file(kimi_agent_file(&request.profile.path));
    }
    if !request.prompt.trim().is_empty() {
        orchestrator = orchestrator.with_initial_task(request.prompt.clone());
    }
    if let Some(session) = request.resume_session.as_deref() {
        orchestrator = orchestrator.with_session(session).with_resume(true);
    }
    Ok(orchestrator)
}

fn worker_models(
    settings: &EffectiveSettings,
    run: &RunRecord,
) -> Result<BTreeMap<crate::Engine, String>> {
    let mut models = crate::settings::process_environment_model_overrides(&settings.config_path)?;
    for (engine, key) in [
        (crate::Engine::Claude, "NEOMAX_CLAUDE_MODEL"),
        (crate::Engine::Codex, "NEOMAX_CODEX_MODEL"),
        (crate::Engine::Opencode, "NEOMAX_OPENCODE_MODEL"),
        (crate::Engine::Kimi, "NEOMAX_KIMI_MODEL"),
        (crate::Engine::Grok, "NEOMAX_GROK_MODEL"),
    ] {
        if let Some(value) = run.environment.get(key) {
            models.insert(
                engine,
                crate::providers::catalog::resolve_model(
                    engine,
                    Some(value),
                    &crate::providers::catalog::MapEnvironment::default(),
                )?
                .id,
            );
        }
    }
    if let Some(values) = run
        .extra
        .get("worker_models")
        .and_then(serde_json::Value::as_object)
    {
        for (engine, key) in [
            (crate::Engine::Claude, "claude"),
            (crate::Engine::Codex, "codex"),
            (crate::Engine::Opencode, "opencode"),
            (crate::Engine::Kimi, "kimi"),
            (crate::Engine::Grok, "grok"),
        ] {
            if let Some(value) = values.get(key).and_then(serde_json::Value::as_str) {
                models.insert(
                    engine,
                    crate::providers::catalog::resolve_model(
                        engine,
                        Some(value),
                        &crate::providers::catalog::MapEnvironment::default(),
                    )?
                    .id,
                );
            }
        }
    }
    Ok(models)
}

#[cfg(test)]
#[path = "tests/prepare/mod.rs"]
mod tests;
