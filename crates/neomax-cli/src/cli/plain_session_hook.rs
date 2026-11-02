//! dev-6w6: Neomax's Claude hooks are installed in the global settings, so every plain `claude` session runs them. A
//! hook does work only inside a Neomax launch, which always carries `NEOMAX_ROLE` (an orchestrator) or
//! `NEOMAX_WORKER` (a worker). Without either, the hook answers nothing, and it answers before runtime discovery, whose
//! process scan alone outlasted the hook's timeout on every prompt.

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};

/// Whether this invocation is one of the installed hooks run outside any Neomax launch.
pub fn is_plain_session_hook(args: &[String]) -> bool {
    let environment = env::vars_os().collect::<BTreeMap<_, _>>();
    plain_session_hook(args, &environment)
}

fn plain_session_hook(args: &[String], environment: &BTreeMap<OsString, OsString>) -> bool {
    let launched = environment.contains_key(OsStr::new("NEOMAX_ROLE"))
        || environment.contains_key(OsStr::new("NEOMAX_WORKER"));
    !launched && is_installed_hook(args)
}

/// The commands the installer writes as hooks: `ls --hook`, `orient --hook`, `usage-hook` and `turn-hook`.
fn is_installed_hook(args: &[String]) -> bool {
    let options = args.split(|arg| arg == "--").next().unwrap_or(args);
    match options.first().map(String::as_str) {
        Some("turn-hook" | "usage-hook") => true,
        Some("orient" | "ls" | "list") => options[1..].iter().any(|arg| arg == "--hook"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn environment(names: &[&str]) -> BTreeMap<OsString, OsString> {
        names
            .iter()
            .map(|name| (OsString::from(name), OsString::from("1")))
            .collect()
    }

    #[test]
    fn a_plain_session_answers_every_installed_hook_before_discovery() {
        let plain = environment(&[]);
        for hook in [
            &["ls", "--hook"][..],
            &["orient", "--hook"],
            &["usage-hook"],
            &["turn-hook"],
        ] {
            assert!(plain_session_hook(&args(hook), &plain), "{hook:?}");
        }
    }

    #[test]
    fn a_neomax_launch_still_runs_its_hooks_and_a_person_still_runs_the_commands() {
        for marker in ["NEOMAX_ROLE", "NEOMAX_WORKER"] {
            assert!(
                !plain_session_hook(&args(&["turn-hook"]), &environment(&[marker])),
                "{marker}"
            );
            assert!(
                !plain_session_hook(&args(&["ls", "--hook"]), &environment(&[marker])),
                "{marker}"
            );
        }
        let plain = environment(&[]);
        assert!(!plain_session_hook(&args(&["ls"]), &plain));
        assert!(!plain_session_hook(&args(&["orient"]), &plain));
        assert!(!plain_session_hook(&args(&["status", "--hook"]), &plain));
        assert!(!plain_session_hook(&args(&["ls", "--", "--hook"]), &plain));
    }
}
