mod accounts;
mod ambient;
mod render;
mod runs;
mod safety;
mod sessions;
mod snapshot;
mod types;

#[cfg(test)]
mod tests;

use anyhow::Result;
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::runs::SystemProcessProbe;

use crate::context::RuntimeContext;
use crate::output;
use crate::parser;

pub(crate) use snapshot::build_report;

pub(crate) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let report = build_report(context).map_err(|_| anyhow::anyhow!("status unavailable"))?;
    if parser::has(args, "--json") {
        return output::json(&report);
    }
    render::text(&report)
}

pub(crate) fn orchestrators(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let records = OrchestratorStore::new(&context.paths.orchestrators)
        .all(&SystemProcessProbe, context.now)
        .map_err(|_| anyhow::anyhow!("orchestrator status unavailable"))?;
    let records = records
        .into_iter()
        .map(ambient::orchestrator)
        .collect::<Vec<_>>();
    if parser::has(args, "--json") {
        return output::json(&records);
    }
    if records.is_empty() {
        println!("no orchestrators registered");
        return Ok(());
    }
    for record in records {
        println!(
            "{} {} account={} live={} pid={}",
            record.session,
            record.engine,
            record.account,
            record.live,
            record.pid.map_or_else(|| "-".into(), |pid| pid.to_string())
        );
    }
    Ok(())
}
