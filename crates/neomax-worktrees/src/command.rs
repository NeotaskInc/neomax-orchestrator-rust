use std::env;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;

use crate::config::RuntimeConfig;
use crate::discovery::resolve;
use crate::git::ProcessGit;
use crate::operations;
use crate::output;

#[derive(Debug, Parser)]
#[command(
    name = "neomax-worktrees",
    version,
    about = "Create, inspect, and safely remove coordinated Git worktree sets"
)]
pub struct Cli {
    #[arg(long, help = "List existing coordinated worktree sets")]
    pub list: bool,
    #[arg(
        long,
        value_name = "TASK",
        help = "Remove one clean coordinated worktree set"
    )]
    pub remove: Option<String>,
    #[arg(
        long,
        help = "Plan the operation without changing Git or the filesystem"
    )]
    pub dry_run: bool,
    #[arg(long, help = "Print machine-readable JSON")]
    pub json: bool,
    #[arg(long, value_name = "PATH", help = "Override the detected project root")]
    pub project_dir: Option<PathBuf>,
    #[arg(
        long,
        value_name = "REPOS",
        help = "Comma or whitespace-separated repository paths"
    )]
    pub repos: Option<String>,
    #[arg(
        long,
        value_name = "PREFIX",
        help = "Override the default branch prefix"
    )]
    pub branch_prefix: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Override coordinated worktree storage"
    )]
    pub worktree_root: Option<PathBuf>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Override the Neomax state directory"
    )]
    pub home: Option<PathBuf>,
    #[arg(long, value_name = "REF", help = "Base ref for newly created branches")]
    pub base: Option<String>,
    #[arg(value_name = "TASK")]
    pub task: Option<String>,
    #[arg(value_name = "BRANCH")]
    pub branch: Option<String>,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    validate_shape(&cli)?;
    let config = RuntimeConfig::from_cli(&cli)?;
    let cwd = env::current_dir()?;
    let git = ProcessGit;
    let context = resolve(&config, &cwd, &git)?;
    if cli.list {
        let report = operations::list(&context, &git)?;
        let rendered = if config.json {
            output::list_json(&report)
        } else {
            output::list_text(&report)
        };
        println!("{rendered}");
        return Ok(());
    }
    if let Some(task) = cli.remove.as_deref() {
        let report = operations::remove_with_base(
            &context,
            task,
            cli.base.as_deref(),
            config.dry_run,
            &git,
        )?;
        let rendered = if config.json {
            output::remove_json(&report, config.dry_run)
        } else {
            output::remove_text(&report, config.dry_run)
        };
        println!("{rendered}");
        return Ok(());
    }
    let task = cli
        .task
        .as_deref()
        .ok_or_else(|| anyhow!("a task slug is required; use --list to inspect sets"))?;
    let report = operations::create(
        &context,
        task,
        cli.branch.as_deref(),
        cli.base.as_deref(),
        config.dry_run,
        &git,
    )?;
    let rendered = if config.json {
        output::create_json(&report, config.dry_run)
    } else {
        output::create_text(&report, config.dry_run)
    };
    println!("{rendered}");
    Ok(())
}

fn validate_shape(cli: &Cli) -> Result<()> {
    if cli.list
        && (cli.remove.is_some()
            || cli.task.is_some()
            || cli.branch.is_some()
            || cli.base.is_some())
    {
        return Err(anyhow!(
            "--list cannot be combined with a task, branch, --base, or --remove"
        ));
    }
    if cli.remove.is_some() && (cli.task.is_some() || cli.branch.is_some()) {
        return Err(anyhow!(
            "--remove cannot be combined with positional task or branch"
        ));
    }
    if cli.branch.is_some() && cli.task.is_none() {
        return Err(anyhow!("a branch requires a task slug"));
    }
    Ok(())
}
