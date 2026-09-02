use anyhow::{Result, bail};
use neomax_core::sessions::{SessionRecord, SessionSummary, portal_snapshot};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;
use crate::parser;

use super::discovery::{DiscoveryOptions, MAX_DISCOVERY_DAYS, discover};
use super::filters::{SessionFilters, parse_engine, validate};
use super::render;

const DEFAULT_DAYS: u32 = 3;
const DEFAULT_SESSION_LIMIT: usize = 60;
const DEFAULT_SUBAGENT_LIMIT: usize = 80;
const MAX_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SessionQueryResult {
    pub days: u32,
    pub project: Option<String>,
    pub engine: Option<neomax_core::Engine>,
    pub active_only: bool,
    pub terminal_only: bool,
    pub records: Vec<SessionRecord>,
    pub summary: SessionSummary,
}

pub(crate) fn parse_options(args: &[String], subagents: bool) -> Result<DiscoveryOptions> {
    let days = parser::value(args, "--days")?
        .map(|value| value.parse::<u32>())
        .transpose()
        .map_err(|_| anyhow::anyhow!("--days must be a positive integer"))?
        .unwrap_or(DEFAULT_DAYS);
    if days == 0 || days > MAX_DISCOVERY_DAYS {
        bail!("--days must be between 1 and {MAX_DISCOVERY_DAYS}");
    }
    let limit = parser::value(args, "--limit")?
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| anyhow::anyhow!("--limit must be a positive integer"))?
        .unwrap_or(if subagents {
            DEFAULT_SUBAGENT_LIMIT
        } else {
            DEFAULT_SESSION_LIMIT
        });
    if limit == 0 || limit > MAX_LIMIT {
        bail!("--limit must be between 1 and {MAX_LIMIT}");
    }
    let project = parser::value(args, "--project")?;
    if project.as_deref().is_some_and(str::is_empty) {
        bail!("--project requires a value");
    }
    let engine = parser::value(args, "--engine")?
        .map(|value| parse_engine(&value))
        .transpose()?;
    let filters = SessionFilters {
        project,
        engine,
        active: parser::has(args, "--active"),
        terminal: parser::has(args, "--terminal"),
    };
    validate(&filters)?;
    let value_flags = ["--days", "--limit", "--project", "--engine"];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if value_flags.contains(&arg.as_str()) {
            if index + 1 >= args.len() {
                bail!("{arg} requires a value");
            }
            index += 2;
            continue;
        }
        if arg.starts_with("--days=")
            || arg.starts_with("--limit=")
            || arg.starts_with("--project=")
            || arg.starts_with("--engine=")
            || matches!(arg.as_str(), "--json" | "--active" | "--terminal")
        {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            bail!("unknown sessions option {arg}");
        }
        bail!("unexpected sessions argument {arg}");
    }
    Ok(DiscoveryOptions {
        days,
        limit,
        filters,
    })
}

pub(crate) fn run_sessions(context: &RuntimeContext, args: &[String]) -> Result<()> {
    run(context, args, false)
}

pub(crate) fn run_subagents(context: &RuntimeContext, args: &[String]) -> Result<()> {
    run(context, args, true)
}

fn run(context: &RuntimeContext, args: &[String], subagents: bool) -> Result<()> {
    let options = error::usage(parse_options(args, subagents))?;
    let inventory = discover(context, &options)?;
    let records = inventory.records(subagents, &options);
    let summary = portal_snapshot(context.now, records.clone()).summary;
    let result = SessionQueryResult {
        days: options.days,
        project: options.filters.project.clone(),
        engine: options.filters.engine,
        active_only: options.filters.active,
        terminal_only: options.filters.terminal,
        records,
        summary,
    };
    if parser::has(args, "--json") {
        return output::json(&result.records);
    }
    render::text(subagents, &result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_filters_for_sessions_and_subagents() {
        let options = parse_options(
            &[
                "--days".into(),
                "14".into(),
                "--limit=9".into(),
                "--project".into(),
                "demo".into(),
                "--engine".into(),
                "grok".into(),
                "--active".into(),
            ],
            false,
        )
        .unwrap();
        assert_eq!(options.days, 14);
        assert_eq!(options.limit, 9);
        assert_eq!(options.filters.project.as_deref(), Some("demo"));
        assert_eq!(options.filters.engine, Some(neomax_core::Engine::Grok));
        assert!(options.filters.active);
        assert_eq!(
            parse_options(&[], true).unwrap().limit,
            DEFAULT_SUBAGENT_LIMIT
        );
    }

    #[test]
    fn rejects_ambiguous_or_unbounded_queries() {
        assert!(parse_options(&["--active".into(), "--terminal".into()], false).is_err());
        assert!(parse_options(&["--days".into(), "0".into()], false).is_err());
        assert!(parse_options(&["--limit".into(), "10001".into()], true).is_err());
        assert!(parse_options(&["typo".into()], false).is_err());
    }
}
