use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::{provider_mode, universal_mode};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::output;
use crate::parser;

pub(super) fn execute(args: &[String], context: &RuntimeContext) -> Result<()> {
    let json_output = parser::has(args, "--json");
    if args.iter().any(|arg| arg != "--json") {
        let unknown = args.iter().find(|arg| arg.as_str() != "--json").unwrap();
        bail!("modes: unknown option {unknown}");
    }
    let modes = std::iter::once(universal_mode())
        .chain(
            Engine::ALL
                .into_iter()
                .map(|engine| provider_mode(engine, neomax_core::WorkerScope::all())),
        )
        .collect::<Vec<_>>();
    let connected = connected_engines(context)?;
    let account_commands = account_commands();
    if json_output {
        return output::json(&json!({
            "modes": modes,
            "connected_engines": connected,
            "account_commands": account_commands,
        }));
    }
    println!("orchestration modes:");
    for mode in &modes {
        println!(
            "  {:<8} {:<10} -> {}",
            mode.command,
            mode.orchestrator
                .map_or_else(|| "dynamic".into(), |engine| engine.to_string()),
            mode.workers
        );
    }
    println!("connected providers: {}", connected.join(", "));
    println!("account helpers: cdx, ocx, kmx, gmx; Claude uses cmax");
    Ok(())
}

fn account_commands() -> Vec<serde_json::Value> {
    Engine::ALL
        .into_iter()
        .map(|engine| match engine {
            Engine::Claude => json!({
                "engine": "claude",
                "helper": "cmax",
                "commands": ["login", "status", "orchestrator"],
                "login": "cmax ACCOUNT (then /login)",
                "orchestrator": "cmax orchestrator (then /login)",
                "reserved": "cmax --orchestrator"
            }),
            Engine::Codex => json!({
                "engine": "codex",
                "helper": "cdx",
                "commands": ["login", "logout", "status", "whoami", "run"],
                "login": "cdx login ACCOUNT [oauth|device|api-key|access-token]",
                "whoami": "cdx whoami [ACCOUNT]",
                "run": "cdx run ACCOUNT [--model MODEL] TASK..."
            }),
            Engine::Opencode => json!({
                "engine": "opencode",
                "helper": "ocx",
                "commands": ["login", "logout", "status", "models", "whoami", "run"],
                "login": "ocx login ACCOUNT [PROVIDER] [oauth|api-key]",
                "models": "ocx models [ACCOUNT] [PROVIDER]",
                "whoami": "ocx whoami [ACCOUNT]",
                "run": "ocx run ACCOUNT [--model MODEL] TASK..."
            }),
            Engine::Kimi => json!({
                "engine": "kimi",
                "helper": "kmx",
                "commands": ["login", "logout", "status", "models", "whoami", "run"],
                "login": "kmx login ACCOUNT [oauth|device|api-key|choose]",
                "whoami": "kmx whoami [ACCOUNT]",
                "run": "kmx run ACCOUNT [--model MODEL] TASK..."
            }),
            Engine::Grok => json!({
                "engine": "grok",
                "helper": "gmx",
                "commands": ["login", "logout", "status", "models", "whoami", "run"],
                "login": "gmx login ACCOUNT [oauth|device|api-key|choose]",
                "whoami": "gmx whoami [ACCOUNT]",
                "run": "gmx run ACCOUNT [--model MODEL] TASK..."
            }),
        })
        .collect()
}

fn connected_engines(context: &RuntimeContext) -> Result<Vec<String>> {
    let runtime = context.provider_runtime()?;
    Ok(Engine::ALL
        .into_iter()
        .filter(|engine| {
            runtime
                .registry()
                .profiles_for(*engine)
                .is_ok_and(|profiles| {
                    profiles
                        .iter()
                        .any(|profile| runtime.registry().managed_pool_eligible(profile))
                })
        })
        .map(|engine| engine.to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn modes_exposes_universal_and_every_provider_without_network() {
        let fixture = fixture();
        execute(&["--json".into()], &fixture.context).unwrap();
    }

    #[test]
    fn account_metadata_matches_the_actual_helper_surfaces() {
        let values = account_commands();
        let claude = &values[0];
        assert_eq!(claude["helper"], "cmax");
        assert_eq!(claude["login"], "cmax ACCOUNT (then /login)");
        assert!(
            claude["commands"]
                .as_array()
                .is_some_and(|commands| !commands.iter().any(|command| command == "whoami"))
        );

        for (index, helper) in ["cdx", "ocx", "kmx", "gmx"].into_iter().enumerate() {
            let metadata = &values[index + 1];
            assert_eq!(metadata["helper"], helper);
            let commands = metadata["commands"].as_array().unwrap();
            assert!(commands.iter().any(|command| command == "whoami"));
            assert!(commands.iter().any(|command| command == "run"));
            assert!(!metadata["login"].as_str().unwrap().contains("--engine"));
        }
    }
}
