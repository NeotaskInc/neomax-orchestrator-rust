use anyhow::Result;
use neomax_core::runs::EventStore;

use super::super::args;
use super::shared::format_timestamp;
use crate::context::RuntimeContext;
use crate::output;

pub(crate) fn audit(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, &["--limit"], &["--json"])?;
    let run_id = parsed.positionals.first().map(String::as_str);
    let limit = parsed
        .value("--limit")
        .map_or(Ok(500usize), |value| args::positive(value, "--limit"))?;
    let events =
        EventStore::with_legacy_directory(&context.paths.run_events, &context.paths.events)
            .read(run_id, limit)?;
    if parsed.has("--json") {
        return output::json(&events);
    }
    if events.is_empty() {
        println!(
            "no audit events recorded{}",
            run_id.map_or_else(String::new, |id| format!(" for {id}"))
        );
        return Ok(());
    }
    for event in events {
        let extra = if event.extra.is_empty() {
            String::new()
        } else {
            format!(" {}", serde_json::to_string(&event.extra)?)
        };
        println!(
            "{}  {:<24} {:<16} {}/{} attempt={}{}",
            format_timestamp(event.ts),
            event.run,
            event.event,
            event.engine,
            event.account.as_deref().unwrap_or("-"),
            event
                .attempt
                .map_or_else(|| "-".into(), |attempt| attempt.to_string()),
            extra,
        );
    }
    Ok(())
}
