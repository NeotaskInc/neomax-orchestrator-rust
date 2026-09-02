use anyhow::Result;

use super::super::args::{self, ParsedArgs};

const VALUE_FLAGS: &[&str] = &[
    "--title",
    "--body",
    "--project",
    "--repos",
    "--severity",
    "--fingerprint",
    "--status",
    "--batch",
    "--run",
    "--pr",
    "--comment",
];
const SWITCH_FLAGS: &[&str] = &["--all", "--json", "--force-new", "--any"];

pub(super) fn parse(args: &[String]) -> Result<(&str, ParsedArgs)> {
    let subcommand = args.first().map(String::as_str).ok_or_else(|| {
        anyhow::anyhow!(
            "usage: neomax issue <open|list|show|next|claim|release|set|link|comment|close|reconcile> ..."
        )
    })?;
    let parsed = args::parse(&args[1..], VALUE_FLAGS, SWITCH_FLAGS)?;
    Ok((subcommand, parsed))
}
