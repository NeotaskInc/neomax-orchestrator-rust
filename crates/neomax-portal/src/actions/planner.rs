use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use neomax_core::io::is_rooted_but_not_absolute;

use super::connect::plan_connect;
use super::executable::{configured_neomax_binary, validate_neomax_binary};
use super::generic::{plan_pause, plan_run};
use crate::model::ActionPlanView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionContext {
    pub home: PathBuf,
    pub state: PathBuf,
    pub neomax_binary: String,
}

impl ActionContext {
    pub fn from_home(home: impl Into<PathBuf>, state: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            state: state.into(),
            neomax_binary: configured_neomax_binary(),
        }
    }

    pub fn from_environment() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("HOME is not set; use --home PATH"))?;
        let state = std::env::var_os("NEOMAX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".neomax"));
        let context = Self::from_home(home, state);
        context.validate_roots()?;
        Ok(context)
    }

    pub(crate) fn validated_neomax_binary(&self) -> Result<String> {
        validate_neomax_binary(&self.neomax_binary)
    }

    pub(crate) fn validate_roots(&self) -> Result<()> {
        for (label, path) in [("action home", &self.home), ("Neomax state", &self.state)] {
            if !path.is_absolute() || is_rooted_but_not_absolute(path) {
                bail!("{label} must be an absolute path")
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionIntent {
    Connect {
        engine: String,
        account: String,
        confirm: bool,
    },
    Pause {
        engine: String,
        account: String,
        paused: bool,
        confirm: bool,
    },
    Run {
        action: ActionKind,
        run_id: String,
        confirm: bool,
    },
}

impl ActionIntent {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Connect { .. } => "connect",
            Self::Pause { paused: true, .. } => "pause",
            Self::Pause { paused: false, .. } => "unpause",
            Self::Run { action, .. } => action.as_str(),
        }
    }

    pub fn confirmed(&self) -> bool {
        match self {
            Self::Connect { confirm, .. }
            | Self::Pause { confirm, .. }
            | Self::Run { confirm, .. } => *confirm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Resume,
    Retry,
    Kill,
    Acknowledge,
    Clean,
}

impl ActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Retry => "retry",
            Self::Kill => "kill",
            Self::Acknowledge => "ack",
            Self::Clean => "clean",
        }
    }

    pub const fn destructive(self) -> bool {
        matches!(self, Self::Kill | Self::Clean)
    }
}

impl std::str::FromStr for ActionKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "resume" => Ok(Self::Resume),
            "retry" => Ok(Self::Retry),
            "kill" => Ok(Self::Kill),
            "ack" | "acknowledge" => Ok(Self::Acknowledge),
            "clean" => Ok(Self::Clean),
            _ => anyhow::bail!("unsupported local action: {value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPlan {
    pub operation: String,
    pub program: String,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub engine: Option<String>,
    pub account: Option<String>,
    pub run_id: Option<String>,
    pub destructive: bool,
    pub confirmation_required: bool,
    pub message: String,
}

impl ActionPlan {
    pub fn view(&self) -> ActionPlanView {
        ActionPlanView {
            operation: self.operation.clone(),
            program: self.program.clone(),
            args: self.args.clone(),
            environment: self.environment.clone(),
            destructive: self.destructive,
            confirmation_required: self.confirmation_required,
            message: self.message.clone(),
        }
    }
}

pub fn plan_action(context: &ActionContext, intent: &ActionIntent) -> Result<ActionPlan> {
    context.validate_roots()?;
    match intent {
        ActionIntent::Connect {
            engine, account, ..
        } => plan_connect(context, engine, account),
        ActionIntent::Pause {
            engine,
            account,
            paused,
            ..
        } => plan_pause(context, engine, account, *paused),
        ActionIntent::Run { action, run_id, .. } => plan_run(context, *action, run_id),
    }
}
