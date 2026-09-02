use std::path::PathBuf;

use neomax_core::Engine;
use neomax_core::orchestration::auth::RotationEffects;
use neomax_core::orchestration::commands::Launcher;
use serde::Serialize;
use serde_json::json;

use super::options::{AuthOptions, launcher_engine};
use super::profiles::rotation_paths;
use crate::context::RuntimeContext;
use crate::output;

#[derive(Debug, Serialize)]
pub(crate) struct AuthReport {
    command: &'static str,
    engine: Engine,
    operation: &'static str,
    destination: Option<PathBuf>,
    source: Option<PathBuf>,
    backup_paths: Vec<PathBuf>,
    invalidated_cache_paths: Vec<PathBuf>,
}

pub(crate) fn print_log(
    launcher: Launcher,
    options: &AuthOptions,
    context: &RuntimeContext,
) -> anyhow::Result<()> {
    let engine = options
        .engine
        .or_else(|| launcher_engine(launcher))
        .or_else(|| std::env::var("NEOMAX_ROLE").ok()?.parse().ok())
        .unwrap_or(Engine::Claude);
    let service =
        neomax_core::orchestration::auth::RotationService::filesystem(rotation_paths(context));
    let events = service
        .recent_rotations(100)?
        .into_iter()
        .filter(|event| event.engine == engine)
        .collect::<Vec<_>>();
    if options.json {
        return output::json(&json!({
            "command": "rotate-auth",
            "engine": engine,
            "operation": "log",
            "events": events,
        }));
    }
    if events.is_empty() {
        println!("rotate-auth: no recorded {engine} rotations");
    } else {
        for event in events {
            println!(
                "{} {} {} -> {} ({})",
                event.ts,
                event.engine,
                event.source.as_deref().unwrap_or("-"),
                event.destination,
                event.reason.as_deref().unwrap_or("manual")
            );
        }
    }
    Ok(())
}

pub(crate) fn print_report(options: &AuthOptions, report: AuthReport) -> anyhow::Result<()> {
    if options.json {
        return output::json(&report);
    }
    println!(
        "rotate-auth: {} {} -> {}",
        report.operation,
        report
            .source
            .as_deref()
            .map_or_else(|| "-".into(), |path| path.display().to_string()),
        report
            .destination
            .as_deref()
            .map_or_else(|| "-".into(), |path| path.display().to_string()),
    );
    Ok(())
}

pub(crate) fn from_effects(
    engine: Engine,
    operation: &'static str,
    effects: RotationEffects,
    destination: Option<PathBuf>,
    source: Option<PathBuf>,
) -> AuthReport {
    AuthReport {
        command: "rotate-auth",
        engine,
        operation,
        destination,
        source,
        backup_paths: effects.backup_paths,
        invalidated_cache_paths: effects.invalidated_cache_paths,
    }
}
