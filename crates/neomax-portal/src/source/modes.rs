use std::env;

use neomax_core::config::{Engine, WorkerScope};
use neomax_core::orchestration::{provider_mode, universal_mode};

use crate::model::{AccountCommandView, ModeView, ModesResponse};

use super::FilesystemPortalSource;

pub(crate) fn available_modes(_source: &FilesystemPortalSource) -> ModesResponse {
    let mut modes = vec![mode(
        universal_mode(),
        Some("Start an orchestrator session".into()),
    )];
    for engine in Engine::ALL {
        modes.push(mode(
            provider_mode(engine, WorkerScope::all()),
            Some("Start an orchestrator session".into()),
        ));
    }
    ModesResponse {
        launch_dir: env::current_dir().ok(),
        modes,
        account_commands: account_commands(),
    }
}

fn mode(mode: neomax_core::orchestration::Mode, section: Option<String>) -> ModeView {
    let why = match mode.orchestrator {
        None => Some(
            "Selects an eligible connected orchestrator and exposes every eligible provider as a worker."
                .into(),
        ),
        Some(Engine::Claude) => Some(
            "Claude uses cmax account semantics: cmax ACCOUNT opens that account, then /login completes authentication."
                .into(),
        ),
        Some(engine) => Some(format!(
            "{} uses its pinned orchestrator with the provider helper's login, logout, status, whoami, and run operations where supported.",
            engine.as_str()
        )),
    };
    ModeView {
        id: mode.id,
        title: mode.title,
        cmd: mode.command,
        orchestrator: mode.orchestrator.map(|engine| engine.as_str().into()),
        workers: mode.workers,
        section,
        why,
    }
}

fn account_commands() -> Vec<AccountCommandView> {
    [
        ("Claude account login", "cmax ACCOUNT (then /login)"),
        (
            "Codex account login",
            "cdx login ACCOUNT [oauth|device|api-key]",
        ),
        ("OpenCode account login", "ocx login ACCOUNT [provider]"),
        (
            "Kimi account login",
            "kmx login ACCOUNT [oauth|device|api-key|choose]",
        ),
        (
            "Grok account login",
            "gmx login ACCOUNT [oauth|device|api-key|choose]",
        ),
        ("Codex account identity", "cdx whoami [ACCOUNT]"),
        ("OpenCode account identity", "ocx whoami [ACCOUNT]"),
        ("Kimi account identity", "kmx whoami [ACCOUNT]"),
        ("Grok account identity", "gmx whoami [ACCOUNT]"),
        (
            "Codex account run",
            "cdx run ACCOUNT [--model MODEL] TASK...",
        ),
        (
            "OpenCode account run",
            "ocx run ACCOUNT [--model MODEL] TASK...",
        ),
        (
            "Kimi account run",
            "kmx run ACCOUNT [--model MODEL] TASK...",
        ),
        (
            "Grok account run",
            "gmx run ACCOUNT [--model MODEL] TASK...",
        ),
        ("Fleet status", "neomax status --json"),
        ("Usage report", "neomax usage --json"),
    ]
    .into_iter()
    .map(|(what, cmd)| AccountCommandView {
        what: what.into(),
        cmd: cmd.into(),
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;

    #[test]
    fn exposes_universal_and_each_provider_mode() {
        let source = FilesystemPortalSource::new("/home/user", "/state");
        let modes = available_modes(&source);
        assert_eq!(modes.modes.len(), 6);
        assert_eq!(modes.modes[0].cmd, "neomax");
        assert!(modes.modes[0].why.is_some());
        assert!(
            modes
                .account_commands
                .iter()
                .any(|row| row.cmd.starts_with("ocx login"))
        );
        assert!(
            modes
                .account_commands
                .iter()
                .all(|row| !row.cmd.contains("--engine"))
        );
        assert!(
            modes
                .account_commands
                .iter()
                .any(|row| row.cmd == "cmax ACCOUNT (then /login)")
        );
        assert!(
            modes
                .account_commands
                .iter()
                .any(|row| row.cmd.starts_with("ocx whoami"))
        );
        assert!(
            modes
                .account_commands
                .iter()
                .any(|row| row.cmd.starts_with("kmx run"))
        );
    }
}
