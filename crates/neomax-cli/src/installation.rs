use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::installation::{
    InstallOptions, InstallPaths, UninstallOptions, install, uninstall,
};

use crate::parser;

mod usage_agent;

pub fn install_command(args: &[String]) -> Result<()> {
    install_command_with_runner(args, &usage_agent::SystemRunner)
}

fn install_command_with_runner(
    args: &[String],
    runner: &dyn usage_agent::CommandRunner,
) -> Result<()> {
    let paths = paths(args)?;
    let report = install(InstallOptions {
        package_root: parser::value(args, "--package-root")?.map(PathBuf::from),
        paths: Some(paths),
        profile_home: None,
        force: parser::has(args, "--force"),
    })?;
    let usage_agent = usage_agent::install_after_transaction(
        &report,
        parser::has(args, "--no-usage-agent") || usage_agent::opted_out_from_environment(),
        runner,
    );
    if let Some(message) = usage_agent::warning(
        "activate the automatic usage and rotation service",
        &usage_agent,
    ) {
        eprintln!("{message}");
    }
    if parser::has(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("installed neomax {}", report.version);
        println!("commands = {}", report.aliases.join(", "));
        println!("auxiliary = {}", report.auxiliaries.join(", "));
        println!("bin = {}", report.bin_dir.display());
        println!("share = {}", report.share_dir.display());
    }
    Ok(())
}

pub fn uninstall_command(args: &[String]) -> Result<()> {
    uninstall_command_with_runner(args, &usage_agent::SystemRunner)
}

fn uninstall_command_with_runner(
    args: &[String],
    runner: &dyn usage_agent::CommandRunner,
) -> Result<()> {
    let paths = paths(args)?;
    let usage_agent = usage_agent::uninstall_before_transaction(&paths, runner);
    if let Some(message) = usage_agent::warning(
        "stop the automatic usage and rotation service",
        &usage_agent,
    ) {
        eprintln!("{message}");
    }
    let report = uninstall(UninstallOptions {
        paths: Some(paths),
        force: parser::has(args, "--force"),
    })?;
    if parser::has(args, "--json") {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("uninstalled neomax");
        println!("removed = {}", report.removed.len());
        if !report.preserved.is_empty() {
            println!("preserved = {}", report.preserved.join(", "));
        }
    }
    Ok(())
}

fn paths(args: &[String]) -> Result<InstallPaths> {
    let root = parser::value(args, "--install-root")?;
    let bin = parser::value(args, "--bin-dir")?;
    let share = parser::value(args, "--share-dir")?;
    if root.is_none() && bin.is_none() && share.is_none() {
        return InstallPaths::discover().map_err(Into::into);
    }
    let root = root.map(PathBuf::from).unwrap_or_else(|| {
        bin.as_ref()
            .and_then(|path| PathBuf::from(path).parent().map(PathBuf::from))
            .or_else(|| {
                share.as_ref().and_then(|path| {
                    PathBuf::from(path)
                        .parent()
                        .and_then(Path::parent)
                        .map(PathBuf::from)
                })
            })
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let bin = bin.map(PathBuf::from).unwrap_or_else(|| root.join("bin"));
    let share = share
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("share/neomax"));
    InstallPaths::new(root, bin, share).map_err(Into::into)
}

pub fn validate_flags(args: &[String]) -> Result<()> {
    let value_flags = [
        "--package-root",
        "--install-root",
        "--bin-dir",
        "--share-dir",
    ];
    let mut value_flag = None;
    for arg in args {
        if let Some(flag) = value_flag.take() {
            if arg.starts_with('-') {
                bail!("{flag} requires a value");
            }
            continue;
        }
        if arg == "--force" || arg == "--json" || arg == "--no-usage-agent" {
            continue;
        }
        if value_flags.iter().any(|flag| arg == *flag) {
            value_flag = value_flags.iter().find(|flag| arg == **flag).copied();
            continue;
        }
        if value_flags
            .iter()
            .any(|flag| arg.starts_with(&format!("{flag}=")))
        {
            continue;
        }
        bail!("unknown installation option {arg}");
    }
    if let Some(flag) = value_flag {
        bail!("{flag} requires a value");
    }
    Ok(())
}
