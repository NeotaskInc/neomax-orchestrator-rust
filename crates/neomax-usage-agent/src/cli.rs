use clap::{Parser, Subcommand};
use serde::Serialize;

use anyhow::Result;

use crate::config::AgentConfig;
use crate::install;
use crate::service::{RunOptions, RunReport, WatchService};

#[derive(Debug, Parser)]
#[command(
    name = "neomax-usage-agent",
    version,
    about = "Collect local Neomax provider usage"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Install,
    Ensure {
        #[arg(long)]
        json: bool,
    },
    Uninstall,
    Status {
        #[arg(long)]
        json: bool,
    },
    Once {
        #[arg(long)]
        rebuild: bool,
        #[arg(long = "no-backfill")]
        no_backfill: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(alias = "watch")]
    Run {
        #[arg(long)]
        rebuild: bool,
        #[arg(long = "no-backfill")]
        no_backfill: bool,
        #[arg(long)]
        once: bool,
        #[arg(long)]
        json: bool,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Status { json: false }) {
        Command::Install => {
            let config = AgentConfig::discover_without_catalog()?;
            print_service(install::install(&config)?, false)
        }
        Command::Ensure { json } => {
            let config = AgentConfig::discover_without_catalog()?;
            print_service(install::ensure(&config)?, json)
        }
        Command::Uninstall => {
            let config = AgentConfig::discover_without_catalog()?;
            print_service(install::uninstall(&config)?, false)
        }
        Command::Status { json } => {
            let config = AgentConfig::discover_without_catalog()?;
            print_service(install::status(&config)?, json)
        }
        Command::Once {
            rebuild,
            no_backfill,
            json,
        } => print_run(
            WatchService::new(AgentConfig::discover()?).run_once(RunOptions {
                rebuild,
                no_backfill,
                once: true,
            })?,
            json,
        ),
        Command::Run {
            rebuild,
            no_backfill,
            once,
            json,
        } => {
            let service = WatchService::new(AgentConfig::discover()?);
            if once {
                return print_run(
                    service.run_once(RunOptions {
                        rebuild,
                        no_backfill,
                        once: true,
                    })?,
                    json,
                );
            }
            if json {
                let report = service.run_once(RunOptions {
                    rebuild,
                    no_backfill,
                    once: false,
                })?;
                print_json(&report)?;
                service.run_forever(RunOptions::default())?;
            } else {
                service.run_forever(RunOptions {
                    rebuild,
                    no_backfill,
                    once: false,
                })?;
            }
            Ok(())
        }
    }
}

fn print_service(report: install::ServiceReport, json: bool) -> Result<()> {
    if json {
        print_json(&report)
    } else {
        println!(
            "neomax-usage-agent: {} ({}) - {}",
            format_state(report.state),
            report.platform,
            report.detail
        );
        Ok(())
    }
}

fn print_run(report: RunReport, json: bool) -> Result<()> {
    if json {
        print_json(&report)
    } else {
        let bootstrap = report
            .bootstrap
            .as_ref()
            .map(|item| item.records_emitted)
            .unwrap_or(0);
        println!(
            "usage-watch: captured {} bootstrap record(s), {} new record(s)",
            bootstrap, report.sweep.records_emitted
        );
        if !report.quota.providers.is_empty() {
            println!(
                "usage-watch: refreshed {} quota profile(s), {} error(s)",
                report
                    .quota
                    .providers
                    .iter()
                    .filter(|provider| provider.refreshed)
                    .count(),
                report.quota.errors
            );
        }
        for maintenance in report.maintenance {
            let outcome = if maintenance.succeeded {
                "ok"
            } else if maintenance.timed_out {
                "timed out"
            } else {
                "failed"
            };
            println!(
                "usage-watch: {} maintenance {}",
                maintenance.action.as_str(),
                outcome
            );
        }
        Ok(())
    }
}

fn format_state(state: install::ServiceState) -> &'static str {
    match state {
        #[cfg(any(target_os = "macos", target_os = "windows", test))]
        install::ServiceState::Loaded => "LOADED",
        #[cfg(any(target_os = "linux", target_os = "windows", test))]
        install::ServiceState::Active => "ACTIVE",
        install::ServiceState::Inactive => "not loaded",
        install::ServiceState::Unsupported => "unsupported",
        install::ServiceState::Unknown => "unknown",
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
