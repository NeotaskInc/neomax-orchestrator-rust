use std::collections::BTreeMap;

use anyhow::{Result, bail};
use neomax_core::runs::{RunStore, SystemProcessProbe, effective_status};
use serde_json::json;

use super::super::args;
use super::shared::{append_event, owned_by_other};
use crate::context::RuntimeContext;
use crate::output;

pub(crate) fn acknowledge(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, &[], &["--all", "--any", "--json"])?;
    let store = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let mut targets = if parsed.has("--all") {
        store
            .all()?
            .into_iter()
            .filter(|run| effective_status(run, &probe).is_terminal() && !run.is_acknowledged())
            .collect::<Vec<_>>()
    } else {
        let id = parsed.positional(0, "ack")?;
        vec![
            store
                .load(id)
                .map_err(anyhow::Error::from)
                .and_then(|run| {
                    if !effective_status(&run, &probe).is_terminal() {
                        bail!("run {id} is not terminal and cannot be acknowledged");
                    }
                    Ok(run)
                })?,
        ]
    };
    let mut skipped = Vec::new();
    let mut acknowledged = Vec::new();
    for run in targets.drain(..) {
        if !parsed.has("--any") && owned_by_other(context, &run)? {
            skipped.push(run.id);
            continue;
        }
        let id = run.id.clone();
        store.update(&id, |current| {
            current.acknowledged = Some(true);
            Ok(())
        })?;
        let mut event_run = run.clone();
        event_run.status = effective_status(&run, &probe);
        append_event(context, &event_run, "acknowledged", BTreeMap::new())?;
        acknowledged.push(id);
    }
    let report = json!({"acknowledged": acknowledged, "skipped_other_owner": skipped});
    if parsed.has("--json") {
        return output::json(&report);
    }
    for id in report["acknowledged"].as_array().into_iter().flatten() {
        println!("acknowledged {}", id.as_str().unwrap_or_default());
    }
    if let Some(skipped) = report["skipped_other_owner"].as_array() {
        if !skipped.is_empty() {
            println!(
                "ack: skipped {} run(s) owned by another live orchestrator; use --any to include them",
                skipped.len()
            );
        }
    }
    Ok(())
}
