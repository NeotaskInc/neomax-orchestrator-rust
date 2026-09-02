use std::env;
use std::ffi::OsString;
use std::path::Path;

use anyhow::{Context, Result};
use neomax_core::orchestration::commands::Launcher;

mod adapters;
mod cli;
mod config;
mod context;
mod error;
mod installation;
mod launch;
mod models;
mod operations;
mod output;
mod parser;
mod process;
mod projects;
mod queue;
mod tasks;

#[cfg(test)]
mod tests;

fn main() {
    let argv0 = env::args_os()
        .next()
        .unwrap_or_else(|| OsString::from("neomax"));
    let invocation = Path::new(&argv0)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("neomax");
    if let Err(error) = run() {
        if let Some(code) = operations::exit_code(&error) {
            eprintln!("{invocation}: {error:#}");
            std::process::exit(code);
        }
        launch::write_startup_error(&error);
        eprintln!("{invocation}: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args_os();
    let argv0 = args.next().unwrap_or_else(|| OsString::from("neomax"));
    let launcher_name = env::var_os("NEOMAX_INVOKED_AS").unwrap_or(argv0);
    let launcher = Launcher::from_argv0(&launcher_name).with_context(|| {
        format!(
            "unsupported invocation name {}",
            launcher_name.to_string_lossy()
        )
    })?;
    let args = error::usage(parser::utf8_args(args.collect()))?;
    cli::authorize_agent_invocation(&args)?;
    if cli::is_version(&args) {
        cli::print_version(launcher);
        return Ok(());
    }
    if cli::is_help(&args) {
        cli::print_help(launcher);
        return Ok(());
    }
    if let Some(command) = args.first().map(String::as_str) {
        if command == "install" {
            return cli::execute_install(&args[1..]);
        }
        if command == "uninstall" {
            return cli::execute_uninstall(&args[1..]);
        }
    }
    let context = context::RuntimeContext::discover()?;
    cli::execute(launcher, &args, &context)
}
