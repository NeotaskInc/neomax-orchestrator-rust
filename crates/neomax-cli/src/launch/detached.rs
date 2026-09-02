use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::providers::scrub_provider_environment;

use crate::context::RuntimeContext;
use crate::output;

use super::{handshake, identity};

pub(crate) fn spawn(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let handshake_path = handshake::create_path(context)?;
    let executable = std::env::current_exe()?;
    // The child must not parse the parent attachment request together with its foreground override.
    let child_args = args
        .iter()
        .filter(|arg| arg.as_str() != "--detach")
        .cloned()
        .collect::<Vec<_>>();
    let mut command = std::process::Command::new(executable);
    scrub_provider_environment(&mut command);
    command.args(&child_args);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if !child_args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--wait" | "--foreground" | "--fg"))
    {
        command.arg("--wait");
    }
    command.env("NEOMAX_ATTACHED_CHILD", "1");
    command.env("NEOMAX_INVOKED_AS", identity::invocation_name(launcher));
    command.env("NEOMAX_LAUNCH_HANDSHAKE", handshake_path.as_os_str());
    let mut child = match crate::process::spawn_detached(&mut command) {
        Ok(child) => child,
        Err(error) => {
            handshake::cleanup(&handshake_path);
            return Err(error);
        }
    };
    let startup = handshake::wait(&handshake_path, &mut child)?;
    if startup.status == "error" {
        let _ = crate::process::terminate_detached(&mut child);
        bail!(
            "{} detached launch failed before startup: {}",
            identity::invocation_name(launcher),
            startup.error.as_deref().unwrap_or("unknown startup error")
        );
    }
    if startup.status != "started" {
        let _ = crate::process::terminate_detached(&mut child);
        bail!(
            "{} detached launch returned unexpected startup status {}",
            identity::invocation_name(launcher),
            startup.status
        );
    }
    let run_id = match startup.run_id.as_deref().filter(|id| !id.trim().is_empty()) {
        Some(run_id) => run_id,
        None => {
            let _ = crate::process::terminate_detached(&mut child);
            bail!("detached launch acknowledged without a run id");
        }
    };
    let message = format!(
        "{} detached run {} started (supervisor pid {})",
        identity::invocation_name(launcher),
        run_id,
        child.id()
    );
    if args.iter().any(|arg| arg == "--json") {
        output::json(&serde_json::json!({
            "invocation": identity::invocation_name(launcher),
            "status": "detached",
            "run_id": run_id,
            "supervisor_pid": child.id(),
        }))
    } else {
        println!("{message}");
        Ok(())
    }
}

pub(crate) fn write_startup_error(error: &anyhow::Error) {
    handshake::write_error(error.to_string());
}
