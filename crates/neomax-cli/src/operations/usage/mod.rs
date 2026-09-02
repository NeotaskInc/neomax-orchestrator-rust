mod query;
mod render;

#[cfg(test)]
mod tests;

use anyhow::Result;

use crate::context::RuntimeContext;
use crate::output;
use crate::parser;

pub(crate) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let (report, _) = query::collect(context, args)?;
    if parser::has(args, "--json") {
        output::json(&report)
    } else {
        render::text(&report)
    }
}
