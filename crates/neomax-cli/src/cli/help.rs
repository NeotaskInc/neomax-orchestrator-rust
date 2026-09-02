use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;

use crate::launch;

pub fn is_help(args: &[String]) -> bool {
    let leading = before_separator(args);
    matches!(
        leading.first().map(String::as_str),
        Some("help" | "commands")
    ) || leading
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help"))
}

pub fn is_version(args: &[String]) -> bool {
    before_separator(args)
        .iter()
        .any(|arg| matches!(arg.as_str(), "--version" | "-V"))
}

fn before_separator(args: &[String]) -> &[String] {
    args.split(|arg| arg == "--").next().unwrap_or(args)
}

pub fn print_help(launcher: Launcher) {
    print!("{}", help_text(launcher));
}

pub fn help_text(launcher: Launcher) -> String {
    const LAUNCH_OPTIONS: &str = "Options:\n  --dry-run [--json]                        inspect without provider execution\n  --engine ENGINE                           claude|codex|opencode|kimi|grok\n  --workers SCOPE                           all or a comma/plus-separated provider subset\n  --model MODEL                             override the selected provider model\n  --claude-model MODEL                      set Claude's local model ID\n  --codex-model MODEL                       set Codex's local model ID\n  --opencode-model MODEL                    set OpenCode's local model ID\n  --kimi-model MODEL                        set Kimi's local model ID\n  --grok-model MODEL                        set Grok's local model ID\n  --prefer ENGINES | --priority ENGINES    provider priority, comma/plus separated\n  --account ACCOUNT                         select a connected account\n  --orchestrator | --dedicated              prefer a reserved orchestrator account\n  --goal TEXT                               attach a verifiable objective\n  --base REF                                use a Git base ref and managed worktree\n  --no-worktree                             run in the current project directory\n  --pr                                      preserve the worktree for pull-request review\n  --plan                                    read-only worker scout in the current checkout\n  --brief                                   acknowledge an intentionally concise worker brief\n  --session-id ID                           set or resume a provider session\n  --resume                                  resume the supplied session\n  --max-turns N                             cap provider correction rounds\n  --wait | --foreground | --fg              stay attached until completion\n  --detach                                  detach a guarded worker dispatch after startup\n  -e LEVEL                                  Claude/Codex effort level after engine selection\n  -t MINUTES                                wall-time limit\n  -s MINUTES                                stall-time limit\n  -n                                        disable automatic account failover\n  -u                                        Claude ultra or Codex xhigh mode\n  --opus                                    explicitly select Claude Opus 5\n  --version | -V                            print the launcher version\n  --help | -h                              show this help\n\nAliases: --priority is --prefer, --dedicated is --orchestrator, --fg is\n--foreground, -V is --version, and -h is --help. A guarded worker dispatch\ndefaults to detached startup when no fixed --run-id or foreground option is\nsupplied. A fixed --run-id stays attached by default so schedulers can own the\ndurable run lifecycle; --detach explicitly overrides that behavior. A root\ninteractive orchestrator stays attached by default. --detach is valid only for\nguarded worker dispatch; use --foreground for a root orchestrator launch.\n\n--plan is valid only for guarded worker dispatch. It uses the current checkout,\nno managed worktree, no write-mode provider permission, and no account or auth\nmutation. Claude uses permission-mode plan, Codex uses read-only sandboxing,\nOpenCode uses its plan agent, Kimi uses a read-only config home and plan\ninstruction, and Grok uses permission-mode plan. A dry-run does not start any\nprovider process; a live plan still requires worker authorization.\n\nPersistent settings:\n  neomax config set max-subagents N\n  neomax config set max-sessions-per-account N\n  neomax config set-model ENGINE MODEL\n  neomax config unset-model ENGINE\n";
    const SOLO_HELP: &str = "\nSolo mode:\n  {name} solo [--model MODEL] [INITIAL_TASK...]\n  {name} --solo [--model MODEL] [INITIAL_TASK...]\n\nSolo mode starts one plain provider session, with no Neomax orchestration injection. The selected profile is copied to the local solo profile when the provider supports it, and the existing rotation state is armed so the session can be continued across an account rotation.\n";
    const RECONCILE_HELP: &str = "\nReconciliation:\n  neomax reconcile [--project NAME] [--limit N] [--heal] [--max N]\n                   [--max-age-hours HOURS] [--allow-repeat] [--any] [--json]\n\n--heal performs bounded, durable resume/retry repair through the normal run lifecycle. Orphaned workers are killed before their preserved run context is resumed. Repairs are recorded in the locked self-heal.json ledger with a five-attempt cap and backoff.\n";
    const PLAN_BOUNDARY: &str = "\nPlan safety: no account or auth mutation. A dry-run does not start any provider process.\n";
    let launch_options = format!(
        "{LAUNCH_OPTIONS}{PLAN_BOUNDARY}  --solo                                    run one plain provider session with local rotation armed\n{RECONCILE_HELP}"
    );
    let launch_options = format!(
        "{launch_options}  --run-id ID                               use a fixed safe durable run ID\n  --tag TEXT                                attach a searchable tag to the run\n"
    );
    let name = launch::invocation_name(launcher);
    let text = match launcher {
        Launcher::Universal => {
            let base = format!(
                "Neomax Orchestrator {}\n\nUsage:\n  {name} [OPTIONS] [INITIAL_TASK...]\n  {name} <COMMAND> [ARGS...]\n\nNeomax dynamically selects among connected providers and routes work across the configured worker scope. A provider alias pins only the orchestrator; --workers remains independent.\n\nCommands:\n  help|commands, config, dispatch\n  projects, project-register, project-unregister\n  task|tasks|backlog, queue\n  select, why, status, list|ls, log, history, sessions, subagents\n  resume, retry, kill, diff, subagent-diff\n  pause, unpause, paused, orchestrators\n  orch-register, orch-unregister, pick-orch, pick-neomax, orch-on\n  rotate, rotate-tick, session-rotate, solo-rotate, solo-setup\n  rotate-auth, handoff, usage, usage-watch, portal, modes, orient\n  keepalive, turn-hook, model-guard, usage-hook\n  run-all, pr, shepherd, premerge, issue, ci-sync\n  reconcile, ack, audit, find, clean, tidy\n  account status|pause|unpause|rotate\n  install, uninstall\n\nProvider aliases:\n  cmax cdxmax ocmax kmax gmax                  pinned orchestrators\n  cdx ocx kmx gmx                              account helpers\n",
                version(),
            );
            let base = format!(
                "{base}\nUniversal auxiliary executables:\n  neomax-portal       combined loopback portal for every provider\n  neomax-usage-agent  local usage and maintenance collector for every provider\n  neomax-worktrees    provider-neutral coordinated Git worktree utility\n"
            );
            format!("{base}{launch_options}")
        }
        Launcher::ProviderOrchestrator(engine) => {
            provider_orchestrator_help(engine, name, &launch_options)
        }
        Launcher::AccountHelper(engine) => account_helper_help(engine, name),
    };
    match launcher {
        Launcher::Universal | Launcher::ProviderOrchestrator(_) => {
            format!("{text}{}", SOLO_HELP.replace("{name}", name))
        }
        Launcher::AccountHelper(_) => text,
    }
}

fn provider_orchestrator_help(engine: Engine, name: &str, launch_options: &str) -> String {
    let claude_setup = if engine == Engine::Claude {
        "\nClaude account setup:\n  cmax N                         open Claude profile N\n  cmax N /login                  pass /login into profile N\n  cmax orchestrator              open .claude-orch, then use /login\n  cmax --orchestrator             use the reserved automatic profile\n"
    } else {
        ""
    };
    let codex_alias = if engine == Engine::Codex {
        "\nCodex model alias: -cm MODEL is equivalent to --codex-model MODEL.\n"
    } else {
        ""
    };
    format!(
        "{name}: {}-pinned Neomax orchestrator {}\n\nUsage:\n  {name} [OPTIONS] [INITIAL_TASK...]\n  {name} resume [SESSION_ID] [OPTIONS]\n  {name} --resume SESSION_ID [OPTIONS]\n  {name} --dry-run [--json] [OPTIONS] [INITIAL_TASK...]\n\nThe orchestrator is pinned to {engine}, while --workers may select any connected provider subset. Without --dry-run, Neomax starts the selected local provider CLI with the managed tool contract. Positional resume and --resume are provider-native interactive session resumes; use the universal neomax resume RUN_ID lifecycle command for managed worker runs.\n\nCommands: status, usage, rotate, portal, orient, usage-watch, keepalive, turn-hook, model-guard, usage-hook, modes, run-all, and the shared lifecycle/workflow commands.\n{claude_setup}{codex_alias}\n{launch_options}",
        engine_label(engine),
        version(),
    )
}

fn account_helper_help(engine: Engine, name: &str) -> String {
    let (login, logout, models, discovery) = match engine {
        Engine::Codex => (
            format!("{name} login ACCOUNT [oauth|device|api-key|access-token]"),
            format!("{name} logout ACCOUNT"),
            None,
            "Codex does not expose a model-listing helper; pass any model supported by the local Codex CLI. OAuth, device authentication, API-key authentication, and stdin access-token login are supported. `cdx login ACCOUNT access-token` maps to `codex login --with-access-token` and reads the token from stdin, so the token is never placed on process arguments or helper output. `status` and `whoami` warn when separate profiles resolve to the same sanitized account identity because their refresh-token family can invalidate both profiles.",
        ),
        Engine::Opencode => (
            format!("{name} login ACCOUNT [PROVIDER] [oauth|api-key]"),
            format!("{name} logout ACCOUNT [PROVIDER]"),
            Some(format!("{name} models [ACCOUNT] [PROVIDER]")),
            "OpenCode uses `opencode auth login --provider PROVIDER` and `opencode auth list`; it discovers models through its local registry and supports provider API-key and OAuth login.",
        ),
        Engine::Kimi => (
            format!("{name} login ACCOUNT [oauth|device [global|mainland-cn]|api-key|choose]"),
            format!("{name} logout ACCOUNT"),
            Some(format!("{name} models [ACCOUNT]")),
            "Kimi discovers models through its local provider registry and supports OAuth, device, and API-key login.",
        ),
        Engine::Grok => (
            format!("{name} login ACCOUNT [oauth|device|api-key|choose]"),
            format!("{name} logout ACCOUNT"),
            Some(format!("{name} models [ACCOUNT]")),
            "Grok discovers models through its local CLI and supports browser OAuth, device OAuth, and API-key login. `choose` prompts for one of those methods. API-key login uses NEOMAX_GROK_API_KEY, XAI_API_KEY, GROK_API_KEY, or GROK_DEPLOYMENT_KEY in that order, then securely prompts when no alias is set.",
        ),
        Engine::Claude => unreachable!("Claude does not have an account-helper launcher"),
    };
    let models_line = models.map_or_else(String::new, |line| format!("  {line}\n"));
    format!(
        "{name}: {} account helper {}\n\nUsage:\n  {login}\n  {logout}\n  {name} status\n{models_line}  {name} whoami [ACCOUNT]\n  {name} run ACCOUNT [OPTIONS] TASK...\n  {name} --dry-run [--json] <operation> [ARGS...]\n\nAuthentication remains owned by the provider CLI. The helper stores account profiles locally. {discovery}\n\nOptions:\n  --dry-run [--json]                        inspect without executing a provider\n  --model MODEL                             select any model ID supported locally\n  --version | -V                            print the launcher version\n  --help | -h                              show this help\n",
        engine_label(engine),
        version(),
    )
}

pub fn print_version(launcher: Launcher) {
    println!("{} {}", launch::invocation_name(launcher), version());
}

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn engine_label(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "Claude",
        Engine::Codex => "Codex",
        Engine::Opencode => "OpenCode",
        Engine::Kimi => "Kimi",
        Engine::Grok => "Grok",
    }
}
