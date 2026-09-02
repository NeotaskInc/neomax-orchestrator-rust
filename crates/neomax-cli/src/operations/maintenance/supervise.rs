use anyhow::{Context, Result, bail};
use neomax_core::runs::{RunStatus, RunStore};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::launch;
use crate::output;
use crate::parser;

pub(super) fn run(args: &[String], context: &RuntimeContext) -> Result<()> {
    let json_output = parser::has(args, "--json");
    let mut positional = Vec::new();
    for arg in args {
        if arg == "--json" {
            continue;
        }
        if arg.starts_with('-') {
            bail!("__supervise: unknown option {arg}");
        }
        positional.push(arg.clone());
    }
    let id = positional.into_iter().next();
    let Some(id) = id else {
        bail!("__supervise requires a run id");
    };
    let store = RunStore::new(&context.paths.runs);
    let mut record = store
        .load(&id)
        .with_context(|| format!("__supervise: unknown run {id}"))?;
    if record.status.is_terminal() {
        return report(&record.id, record.status, json_output);
    }
    record.supervisor_pid = Some(std::process::id());
    store.save_preserving_control_markers(&record)?;
    let runtime = context.provider_runtime()?;
    let status = launch::execute_record_with_runtime(
        &runtime,
        &context.paths,
        &context.settings,
        &mut record,
    )?;
    report(&record.id, status, json_output)
}

fn report(id: &str, status: RunStatus, json_output: bool) -> Result<()> {
    if json_output {
        return output::json(&json!({
            "command": "__supervise",
            "run_id": id,
            "status": status.as_str(),
        }));
    }
    println!("__supervise: run {id} {}", status.as_str());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn missing_run_fails_closed_before_provider_discovery() {
        let fixture = fixture();
        let error = run(&["missing".into()], &fixture.context).unwrap_err();
        assert!(error.to_string().contains("unknown run missing"));
    }
}
