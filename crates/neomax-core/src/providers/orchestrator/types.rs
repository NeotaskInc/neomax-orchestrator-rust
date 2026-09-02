use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::WorkerScope;
use crate::providers::{catalog, ProviderProcessSecret, ProviderProfile};
use crate::Engine;

pub const ORCHESTRATOR_INSTRUCTION_ENV: &str = "NEOMAX_ORCHESTRATOR_INSTRUCTION";
pub const ORCHESTRATOR_ORIENTATION_ENV: &str = "NEOMAX_ORCHESTRATOR_ORIENTATION";

/// Kimi loads the system instructions for a new interactive session from a
/// Markdown agent definition. The installer or launch layer owns the file's
/// lifecycle; the command builder only receives its explicit path.
pub const KIMI_AGENT_FILE_RELATIVE_PATH: &str = "agents/neomax.md";

pub fn kimi_agent_file(profile: &Path) -> PathBuf {
    profile.join("agents").join("neomax.md")
}

/// The environment supplied to an interactive provider session.
///
/// The values are explicit so command construction never has to inspect the
/// host environment. `variables` is intended for safe Neomax/tool variables;
/// credential-shaped keys are rejected by the builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorEnvironment {
    pub fleet: WorkerScope,
    pub session: String,
    pub pid: Option<u32>,
    pub reserved: bool,
    pub worker_models: BTreeMap<Engine, String>,
    pub variables: BTreeMap<String, String>,
}

impl OrchestratorEnvironment {
    pub fn new(fleet: WorkerScope, session: impl Into<String>) -> Self {
        let worker_models = Engine::ALL
            .into_iter()
            .map(|engine| (engine, catalog::default_model_id(engine).into()))
            .collect();
        Self {
            fleet,
            session: session.into(),
            pid: None,
            reserved: false,
            worker_models,
            variables: BTreeMap::new(),
        }
    }

    pub fn with_model(mut self, engine: Engine, model: impl Into<String>) -> Self {
        self.worker_models.insert(engine, model.into());
        self
    }

    pub fn with_variable(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(key.into(), value.into());
        self
    }

    pub const fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    pub const fn reserved(mut self, reserved: bool) -> Self {
        self.reserved = reserved;
        self
    }
}

/// Inputs for a provider's main interactive orchestrator process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorRequest {
    pub profile: ProviderProfile,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub goal: Option<String>,
    pub max_turns: Option<u32>,
    pub effort: Option<String>,
    pub ultra: bool,
    pub solo: bool,
    pub initial_task: Option<String>,
    pub agent_file: Option<PathBuf>,
    pub session_id: Option<String>,
    pub resume: bool,
    pub environment: OrchestratorEnvironment,
    pub orientation: Option<String>,
    pub(crate) process_secret: Option<ProviderProcessSecret>,
}

impl OrchestratorRequest {
    pub fn new(
        profile: ProviderProfile,
        home: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        environment: OrchestratorEnvironment,
    ) -> Self {
        Self {
            profile,
            home: home.into(),
            cwd: cwd.into(),
            model: None,
            goal: None,
            max_turns: None,
            effort: None,
            ultra: false,
            solo: false,
            initial_task: None,
            agent_file: None,
            session_id: None,
            resume: false,
            environment,
            orientation: None,
            process_secret: None,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        self.goal = Some(goal.into());
        self
    }

    pub const fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = Some(max_turns);
        self
    }

    pub fn with_effort(mut self, effort: impl Into<String>) -> Self {
        self.effort = Some(effort.into());
        self
    }

    pub const fn with_ultra(mut self, ultra: bool) -> Self {
        self.ultra = ultra;
        self
    }

    pub const fn with_solo(mut self, solo: bool) -> Self {
        self.solo = solo;
        self
    }

    pub fn with_initial_task(mut self, task: impl Into<String>) -> Self {
        self.initial_task = Some(task.into());
        self
    }

    pub fn with_orientation(mut self, orientation: impl Into<String>) -> Self {
        self.orientation = Some(orientation.into());
        self
    }

    pub fn instruction(&self) -> &str {
        self.orientation
            .as_deref()
            .unwrap_or(crate::providers::worker::ORCHESTRATOR_DIRECTIVE)
    }

    pub fn with_agent_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.agent_file = Some(path.into());
        self
    }

    pub fn with_session(mut self, session: impl Into<String>) -> Self {
        self.session_id = Some(session.into());
        self
    }

    pub const fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }

    pub(crate) fn with_process_secret(mut self, secret: Option<ProviderProcessSecret>) -> Self {
        self.process_secret = secret;
        self
    }
}
