use std::cmp::Reverse;

use anyhow::{Result, bail};
use neomax_core::runs::RunStore;

use super::super::args;
use super::shared::{run_match, searchable_fields};
use crate::context::RuntimeContext;
use crate::output;

pub(crate) fn find(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, &[], &["--json"])?;
    let needle = parsed.positionals.join(" ").to_ascii_lowercase();
    if needle.trim().is_empty() {
        bail!("find requires a keyword or path");
    }
    let mut runs = RunStore::new(&context.paths.runs).all()?;
    runs.sort_by_key(|run| Reverse(run.started));
    let matches = runs
        .into_iter()
        .filter_map(|run| {
            let haystack = searchable_fields(&run).join("\n");
            haystack
                .to_ascii_lowercase()
                .contains(&needle)
                .then(|| run_match(&run))
        })
        .collect::<Vec<_>>();
    if parsed.has("--json") {
        return output::json(&matches);
    }
    if matches.is_empty() {
        println!("no prior run matched {needle:?}");
        return Ok(());
    }
    println!("prior runs matching {needle:?}:");
    for run in matches {
        println!(
            "  {} [{} / {}] status={} session={}",
            run.id,
            run.engine,
            run.account,
            run.status,
            run.session.as_deref().unwrap_or("-")
        );
        println!(
            "      prompt: {}",
            run.prompt.chars().take(120).collect::<String>()
        );
        if !run.files.is_empty() {
            println!("      touched: {}", run.files.join(", "));
        }
    }
    Ok(())
}
