use std::ffi::OsStr;
use std::path::Path;

use crate::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launcher {
    Universal,
    ProviderOrchestrator(Engine),
    AccountHelper(Engine),
}

impl Launcher {
    pub fn from_argv0(argv0: &OsStr) -> Option<Self> {
        let name = Path::new(argv0).file_name()?.to_str()?;
        let name = name.to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name);
        match name {
            "neomax" | "neomax-cli" => Some(Self::Universal),
            "cmax" => Some(Self::ProviderOrchestrator(Engine::Claude)),
            "cdxmax" => Some(Self::ProviderOrchestrator(Engine::Codex)),
            "ocmax" => Some(Self::ProviderOrchestrator(Engine::Opencode)),
            "kmax" => Some(Self::ProviderOrchestrator(Engine::Kimi)),
            "gmax" => Some(Self::ProviderOrchestrator(Engine::Grok)),
            "cdx" => Some(Self::AccountHelper(Engine::Codex)),
            "ocx" => Some(Self::AccountHelper(Engine::Opencode)),
            "kmx" => Some(Self::AccountHelper(Engine::Kimi)),
            "gmx" => Some(Self::AccountHelper(Engine::Grok)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Help,
    Select,
    Why,
    Config,
    Dispatch,
    List,
    Log,
    Resume,
    Retry,
    Kill,
    PullRequest,
    Reconcile,
    Acknowledge,
    Audit,
    Find,
    History,
    Status,
    Pause,
    Unpause,
    Paused,
    Orchestrators,
    OrchestratorRegister,
    OrchestratorUnregister,
    Premerge,
    PickOrchestrator,
    PickNeomax,
    OrchestratorOn,
    Solo,
    SoloRotate,
    SoloSetup,
    SessionRotate,
    Rotate,
    RotateTick,
    Handoff,
    Modes,
    Sessions,
    Subagents,
    Diff,
    SubagentDiff,
    Projects,
    ProjectRegister,
    ProjectUnregister,
    Tasks,
    RotateAuth,
    Orient,
    Usage,
    UsageWatch,
    Keepalive,
    TurnHook,
    ModelGuard,
    UsageHook,
    Portal,
    RunAll,
    Shepherd,
    Issue,
    CiSync,
    Queue,
    Clean,
    Tidy,
    Install,
    Uninstall,
    Supervise,
}

impl Command {
    pub const fn canonical_name(self) -> Option<&'static str> {
        Some(match self {
            Self::Help => "help",
            Self::Select => "select",
            Self::Why => "why",
            Self::Config => "config show",
            Self::Dispatch => "dispatch",
            Self::List => "ls",
            Self::Log => "log",
            Self::Resume => "resume",
            Self::Retry => "retry",
            Self::Kill => "kill",
            Self::PullRequest => "pr",
            Self::Reconcile => "reconcile",
            Self::Acknowledge => "ack",
            Self::Audit => "audit",
            Self::Find => "find",
            Self::History => "history",
            Self::Status => "status",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Paused => "paused",
            Self::Orchestrators => "orchestrators",
            Self::OrchestratorRegister => "orch-register",
            Self::OrchestratorUnregister => "orch-unregister",
            Self::Premerge => "premerge",
            Self::PickOrchestrator => "pick-orch",
            Self::PickNeomax => "pick-neomax",
            Self::OrchestratorOn => "orch-on",
            Self::Solo => "solo",
            Self::SoloRotate => "solo-rotate",
            Self::SoloSetup => "solo-setup",
            Self::SessionRotate => "session-rotate",
            Self::Rotate => "rotate",
            Self::RotateTick => "rotate-tick",
            Self::Handoff => "handoff",
            Self::Modes => "modes",
            Self::Sessions => "sessions",
            Self::Subagents => "subagents",
            Self::Diff => "diff",
            Self::SubagentDiff => "subagent-diff",
            Self::Projects => "projects",
            Self::ProjectRegister => "project-register",
            Self::ProjectUnregister => "project-unregister",
            Self::Tasks => "tasks",
            Self::RotateAuth => "rotate-auth",
            Self::Orient => "orient",
            Self::Usage => "usage",
            Self::UsageWatch => "usage-watch",
            Self::Keepalive => "keepalive",
            Self::TurnHook => "turn-hook",
            Self::ModelGuard => "model-guard",
            Self::UsageHook => "usage-hook",
            Self::Portal => "portal",
            Self::RunAll => "run-all",
            Self::Shepherd => "shepherd",
            Self::Issue => "issue",
            Self::CiSync => "ci-sync",
            Self::Queue => "queue",
            Self::Clean => "clean",
            Self::Tidy => "tidy",
            Self::Install => "install",
            Self::Uninstall => "uninstall",
            Self::Supervise => return None,
        })
    }
}

pub fn resolve(name: &str) -> Option<Command> {
    Some(match name {
        "help" | "commands" | "-h" | "--help" => Command::Help,
        "select" => Command::Select,
        "why" => Command::Why,
        "config" => Command::Config,
        "dispatch" | "delegate" | "auto" => Command::Dispatch,
        "list" | "ls" => Command::List,
        "log" => Command::Log,
        "resume" => Command::Resume,
        "retry" => Command::Retry,
        "kill" => Command::Kill,
        "pr" => Command::PullRequest,
        "reconcile" => Command::Reconcile,
        "ack" => Command::Acknowledge,
        "audit" => Command::Audit,
        "find" => Command::Find,
        "history" => Command::History,
        "status" => Command::Status,
        "pause" => Command::Pause,
        "unpause" => Command::Unpause,
        "paused" => Command::Paused,
        "orchestrators" | "orch-list" | "orchs" => Command::Orchestrators,
        "orch-register" | "orch_register" => Command::OrchestratorRegister,
        "orch-unregister" | "orch_unregister" => Command::OrchestratorUnregister,
        "premerge-check" | "premerge" => Command::Premerge,
        "pick-orch" | "pick_orch" => Command::PickOrchestrator,
        "pick-neomax" | "pick_neomax" => Command::PickNeomax,
        "orch-on" | "orch_on" => Command::OrchestratorOn,
        "solo" => Command::Solo,
        "solo-rotate" | "solo_rotate" => Command::SoloRotate,
        "solo-setup" | "solo_setup" => Command::SoloSetup,
        "session-rotate" | "session_rotate" => Command::SessionRotate,
        "rotate" => Command::Rotate,
        "rotate-tick" | "rotate_tick" => Command::RotateTick,
        "handoff" => Command::Handoff,
        "modes" => Command::Modes,
        "sessions" => Command::Sessions,
        "subagents" => Command::Subagents,
        "diff" => Command::Diff,
        "subagent-diff" | "subagent_diff" => Command::SubagentDiff,
        "projects" => Command::Projects,
        "project-register" | "project_register" | "register-project" => Command::ProjectRegister,
        "project-unregister" | "project_unregister" | "unregister-project" => {
            Command::ProjectUnregister
        }
        "task" | "tasks" | "backlog" => Command::Tasks,
        "rotate-auth" | "rotate_auth" => Command::RotateAuth,
        "orient" => Command::Orient,
        "usage" => Command::Usage,
        "usage-watch" | "usage_watch" => Command::UsageWatch,
        "keepalive" | "keep-alive" => Command::Keepalive,
        "turn-hook" | "turn_hook" => Command::TurnHook,
        "model-guard" | "model_guard" => Command::ModelGuard,
        "usage-hook" | "usage_hook" => Command::UsageHook,
        "portal" => Command::Portal,
        "run-all" | "runall" => Command::RunAll,
        "shepherd" => Command::Shepherd,
        "issue" => Command::Issue,
        "ci-sync" => Command::CiSync,
        "queue" => Command::Queue,
        "clean" => Command::Clean,
        "tidy" => Command::Tidy,
        "install" => Command::Install,
        "uninstall" => Command::Uninstall,
        "__supervise" => Command::Supervise,
        value if value.parse::<u32>().is_ok() => Command::Dispatch,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_multicall_name_selects_the_expected_surface() {
        assert_eq!(
            Launcher::from_argv0(OsStr::new("ocmax")),
            Some(Launcher::ProviderOrchestrator(Engine::Opencode))
        );
        assert_eq!(
            Launcher::from_argv0(OsStr::new("kmx")),
            Some(Launcher::AccountHelper(Engine::Kimi))
        );
        assert_eq!(
            Launcher::from_argv0(OsStr::new("neomax")),
            Some(Launcher::Universal)
        );
        assert_eq!(
            Launcher::from_argv0(OsStr::new("ocmax.exe")),
            Some(Launcher::ProviderOrchestrator(Engine::Opencode))
        );
        assert_eq!(
            Launcher::from_argv0(OsStr::new("OCMAX.EXE")),
            Some(Launcher::ProviderOrchestrator(Engine::Opencode))
        );
        assert_eq!(
            Launcher::from_argv0(OsStr::new("NEOMAX.EXE")),
            Some(Launcher::Universal)
        );
    }

    #[test]
    fn preserves_public_and_internal_aliases() {
        assert_eq!(resolve("orch-list"), Some(Command::Orchestrators));
        assert_eq!(resolve("list"), Some(Command::List));
        assert_eq!(resolve("ls"), Some(Command::List));
        assert_eq!(resolve("dispatch"), Some(Command::Dispatch));
        assert_eq!(
            resolve("project_unregister"),
            Some(Command::ProjectUnregister)
        );
        assert_eq!(resolve("12"), Some(Command::Dispatch));
        assert_eq!(resolve("commands"), Some(Command::Help));
    }
}
