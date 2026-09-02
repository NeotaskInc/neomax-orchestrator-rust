use anyhow::{Result, bail};
use neomax_core::Engine;
use serde_json::json;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;
use crate::parser;

pub(super) fn run(args: &[String], context: &RuntimeContext) -> Result<()> {
    error::usage(validate_args(args))?;
    let checked = checked_profiles(context);
    let refreshed = 0usize;
    if parser::has(args, "--json") {
        return output::json(&json!({
            "command": "keepalive",
            "checked": checked,
            "refreshed": refreshed,
            "mode": "local",
        }));
    }
    println!("keepalive: checked {checked} local account profile(s); refreshed {refreshed}");
    Ok(())
}

fn validate_args(args: &[String]) -> Result<()> {
    for arg in args {
        if !matches!(arg.as_str(), "--once" | "--json") {
            bail!("keepalive: unknown option {arg}");
        }
    }
    Ok(())
}

fn checked_profiles(context: &RuntimeContext) -> usize {
    let Ok(runtime) = context.provider_runtime() else {
        return 0;
    };
    Engine::ALL
        .into_iter()
        .filter_map(|engine| runtime.registry().profiles_for(engine).ok())
        .flatten()
        .filter(|profile| runtime.registry().worker_eligible(profile))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn once_is_a_local_noop_when_no_profiles_are_discovered() {
        let fixture = fixture();
        run(&["--once".into(), "--json".into()], &fixture.context).unwrap();
    }

    #[test]
    fn rejects_provider_or_network_options() {
        let fixture = fixture();
        assert!(run(&["--refresh".into()], &fixture.context).is_err());
    }
}
