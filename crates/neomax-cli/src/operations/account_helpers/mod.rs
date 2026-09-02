mod actions;
mod commands;
mod process;
mod profiles;
mod prompt;
mod render;
mod request;

#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;

use crate::context::RuntimeContext;
use crate::error;

use self::process::{LocalProcessPort, ProcessPort};
use self::profiles::{AuthPort, FileAuthPort};
use self::request::AccountHelperRequest;

pub(crate) fn run(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let engine = match launcher {
        Launcher::AccountHelper(engine) => engine,
        _ => bail!("account helpers require cdx, ocx, kmx, or gmx"),
    };
    let auth = FileAuthPort;
    let process = LocalProcessPort;
    run_with_ports(engine, args, context, &auth, &process)
}

pub(crate) fn run_with_ports(
    engine: Engine,
    args: &[String],
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    let request = error::usage(AccountHelperRequest::parse(engine, args))?;
    actions::dispatch(&request, context, auth, process)
}
