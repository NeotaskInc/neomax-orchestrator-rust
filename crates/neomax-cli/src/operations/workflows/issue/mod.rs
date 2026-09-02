mod mutate;
mod open;
mod parser;
mod read;
mod render;
mod service;

#[cfg(test)]
mod tests;

use anyhow::{Result, bail};

use crate::context::RuntimeContext;

pub(super) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let (subcommand, parsed) = parser::parse(args)?;
    match subcommand {
        "open" => open::run(context, &parsed),
        "list" => read::list(context, &parsed),
        "show" => read::show(context, &parsed),
        "next" => read::next(context, &parsed),
        "claim" => mutate::claim(context, &parsed),
        "release" => mutate::release(context, &parsed),
        "set" => mutate::set_status(context, &parsed),
        "link" => mutate::link(context, &parsed),
        "comment" => mutate::comment(context, &parsed),
        "close" => mutate::close(context, &parsed),
        "reconcile" => mutate::reconcile(context, &parsed),
        other => bail!("unknown issue subcommand {other:?}"),
    }
}
