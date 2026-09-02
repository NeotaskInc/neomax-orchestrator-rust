use anyhow::Result;
use neomax_core::orchestration::commands::Launcher;
use serde_json::{Value, json};

use crate::launch::RotationReport;
use crate::output;
use crate::parser;

pub(super) fn reports(
    launcher: Launcher,
    command: &str,
    args: &[String],
    reports: Vec<RotationReport>,
    mut extra: Value,
) -> Result<()> {
    if parser::has(args, "--json") {
        let object = extra
            .as_object_mut()
            .expect("rotation report metadata must be a JSON object");
        object.insert("command".into(), json!(command));
        object.insert(
            "invocation".into(),
            json!(crate::launch::invocation_name(launcher)),
        );
        object.insert("rotated".into(), json!(reports));
        return output::json(&extra);
    }
    for report in reports {
        let crossing = if report.crosses_provider {
            " cross-provider"
        } else {
            ""
        };
        println!(
            "{}{}: {} {} -> {} {} attempt {}",
            report.run_id,
            crossing,
            report.status,
            report.source_engine,
            report.target_engine.as_deref().unwrap_or("-"),
            report.target_account.as_deref().unwrap_or("-"),
            report.attempt
        );
    }
    Ok(())
}

pub(super) fn no_op(command: &str, args: &[String], detail: &str) -> Result<()> {
    if parser::has(args, "--json") {
        return output::json(&json!({
            "command": command,
            "status": "no-op",
            "detail": detail,
            "rotated": [],
        }));
    }
    println!("{command}: {detail}");
    Ok(())
}
