use anyhow::Result;

use super::query::SessionQueryResult;

pub(crate) fn text(subagents: bool, result: &SessionQueryResult) -> Result<()> {
    let kind = if subagents {
        "NATIVE SUBAGENTS"
    } else {
        "INTERACTIVE SESSIONS"
    };
    println!(
        "{kind} | last {}d | records={} active={} working={} input={} output={} reasoning={} cost=${:.2}",
        result.days,
        result.summary.sessions,
        result.summary.active,
        result.summary.working,
        result.summary.input,
        result.summary.output,
        result.summary.reasoning,
        result.summary.cost,
    );
    if let Some(project) = result.project.as_deref() {
        println!("project: {project}");
    }
    if let Some(engine) = result.engine {
        println!("provider: {engine}");
    }
    if result.records.is_empty() {
        println!("none");
        return Ok(());
    }
    for record in &result.records {
        let state = if record.active || record.working {
            "ACTIVE"
        } else if record.done || record.archived {
            "done"
        } else {
            "idle"
        };
        let label = record
            .label
            .as_deref()
            .or(record.slug.as_deref())
            .unwrap_or("");
        println!(
            "  {:<12} {:<8} acct={:<6} {:<12} {} input={} output={} tools={} {}",
            record.engine,
            state,
            record.account,
            short_id(&record.id),
            record.project.as_deref().unwrap_or("-"),
            record.tokens.input,
            record.tokens.output,
            record.tool_calls,
            label,
        );
    }
    Ok(())
}

fn short_id(value: &str) -> &str {
    value.get(..8).unwrap_or(value)
}
