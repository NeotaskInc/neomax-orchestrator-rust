use anyhow::{Result, bail};
use neomax_core::orchestration::commands::Launcher;
use neomax_core::runs::execution::MAX_TIMEOUT_MINUTES;
use neomax_core::{Engine, WorkerScope};
use std::env;

use crate::models::{parse_engine, validate_model};

use super::types::LaunchOptions;
use super::validation;

impl LaunchOptions {
    pub(crate) fn parse(launcher: Launcher, args: &[String]) -> Result<Self> {
        let mut options = Self {
            routing_allowed: true,
            detach: false,
            foreground: !matches!(launcher, Launcher::AccountHelper(_)),
            wall_min: Some(240.0),
            no_worktree: env::var("NEOMAX_NO_WORKTREE")
                .ok()
                .is_some_and(|value| !value.is_empty()),
            ..Self::default()
        };
        let mut attachment_explicit = false;
        let mut detach_requested = false;
        let mut foreground_requested = false;
        let mut forced_worker_dispatch = false;
        let mut solo_forbidden_flag = None;
        let mut after_separator = false;
        let mut index = 0;
        while index < args.len() {
            let current = args[index].as_str();
            if !after_separator && current == "--" {
                after_separator = true;
                options.routing_allowed = false;
                index += 1;
                continue;
            }
            if !after_separator && current.starts_with('-') {
                let (flag, inline) = split_flag(current);
                match flag {
                    "--dry-run" => options.dry_run = true,
                    "--json" => {}
                    "-n" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.no_failover = true;
                    }
                    "--wait" | "--foreground" | "--fg" => {
                        options.detach = false;
                        options.foreground = true;
                        attachment_explicit = true;
                        foreground_requested = true;
                    }
                    "--detach" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.detach = true;
                        options.foreground = false;
                        attachment_explicit = true;
                        detach_requested = true;
                    }
                    "--plan" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.plan_mode = true;
                        options.no_worktree = true;
                    }
                    "--no-worktree" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.no_worktree = true;
                    }
                    "--pr" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.open_pull_request = true;
                    }
                    "--brief" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.brief = true;
                    }
                    "-u" => options.ultra = true,
                    "--opus" => {
                        options.opus = true;
                        if matches!(launcher, Launcher::Universal) {
                            options.engine = Some(Engine::Claude);
                        }
                    }
                    "--resume" => {
                        options.resume = true;
                        if let Some(session) = inline {
                            if session.is_empty() {
                                bail!("--resume requires a session ID");
                            }
                            options.session_id = Some(session.to_owned());
                        }
                    }
                    "--worker-dispatch" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        forced_worker_dispatch = true;
                    }
                    "--solo" => options.solo = true,
                    "--orchestrator" | "--dedicated" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.dedicated = true;
                    }
                    "--engine" => {
                        options.engine =
                            Some(parse_engine(&value(args, &mut index, flag, inline)?)?);
                    }
                    "--workers" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.worker_scope =
                            Some(value(args, &mut index, flag, inline)?.parse::<WorkerScope>()?);
                    }
                    "--model" => {
                        options.model =
                            Some(validate_model(value(args, &mut index, flag, inline)?)?);
                    }
                    "--claude-model" | "--codex-model" | "-cm" | "--opencode-model"
                    | "--kimi-model" | "--grok-model" => {
                        let engine = engine_model_flag(flag).expect("known model flag");
                        options.provider_models.insert(
                            engine,
                            validate_model(value(args, &mut index, flag, inline)?)?,
                        );
                    }
                    "--goal" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.goal = Some(value(args, &mut index, flag, inline)?);
                    }
                    "--base" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.base = Some(value(args, &mut index, flag, inline)?);
                    }
                    "--run-id" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.run_id = Some(validation::validate_run_id(&value_allow_empty(
                            args, &mut index, flag, inline,
                        )?)?);
                    }
                    "--tag" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.tag = Some(validation::validate_tag(&value_allow_empty(
                            args, &mut index, flag, inline,
                        )?)?);
                    }
                    "--session-id" => {
                        options.session_id = Some(value(args, &mut index, flag, inline)?);
                    }
                    "--max-turns" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        let raw = value(args, &mut index, flag, inline)?;
                        let turns = raw
                            .parse::<u32>()
                            .map_err(|_| anyhow::anyhow!("{flag} requires a positive integer"))?;
                        if turns == 0 {
                            bail!("{flag} requires a positive integer");
                        }
                        options.max_turns = Some(turns);
                    }
                    "--prefer" | "--priority" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.priority = Some(value(args, &mut index, flag, inline)?);
                    }
                    "--account" => {
                        let account = value(args, &mut index, flag, inline)?;
                        if account.is_empty() {
                            bail!("{flag} requires a value");
                        }
                        options.account = Some(account);
                        options.routing = "account".into();
                    }
                    "-e" => {
                        let effort = value(args, &mut index, flag, inline)?;
                        if !matches!(effort.as_str(), "low" | "medium" | "high" | "xhigh" | "max") {
                            bail!("-e requires low, medium, high, xhigh, or max");
                        }
                        options.effort = Some(effort);
                    }
                    "-t" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.wall_min = Some(minutes(args, &mut index, flag, inline)?);
                    }
                    "-s" => {
                        mark_solo_forbidden(&mut solo_forbidden_flag, flag);
                        options.stall_min = Some(minutes(args, &mut index, flag, inline)?);
                    }
                    _ => bail!("unknown launch option {current}"),
                }
                index += 1;
                continue;
            }
            options.positionals.push(current.to_owned());
            index += 1;
        }

        if detach_requested && foreground_requested {
            bail!("--detach and --foreground cannot be used together");
        }

        if matches!(launcher, Launcher::AccountHelper(_)) {
            let helper_command = options.positionals.first().and_then(|command| {
                matches!(
                    command.as_str(),
                    "login" | "logout" | "orchestrator" | "orch" | "models" | "status" | "run"
                )
                .then_some(command.clone())
            });
            if helper_command.is_some() {
                options.helper_command = Some(options.positionals.remove(0));
                options.helper_args = std::mem::take(&mut options.positionals);
                if options
                    .helper_args
                    .first()
                    .is_some_and(|value| value.parse::<u32>().is_ok())
                {
                    options.routing = "account".into();
                    options.account = Some(options.helper_args.remove(0));
                } else {
                    options.routing = "default".into();
                }
            }
        }
        if matches!(launcher, Launcher::ProviderOrchestrator(Engine::Claude))
            && options.routing_allowed
            && options.account.is_none()
            && options.positionals.first().is_some_and(|value| {
                value.eq_ignore_ascii_case("orchestrator") || value.eq_ignore_ascii_case("orch")
            })
        {
            options.positionals.remove(0);
            options.account = Some("orch".into());
            options.routing = "account".into();
        }
        let mut worker_dispatch = forced_worker_dispatch;
        if !matches!(launcher, Launcher::AccountHelper(_)) || options.helper_command.is_none() {
            if options.routing_allowed {
                if options.account.is_some() {
                    options.routing = "account".into();
                } else if let Some(first) = options.positionals.first() {
                    if first.eq_ignore_ascii_case("auto") {
                        options.routing = "auto".into();
                        options.positionals.remove(0);
                        worker_dispatch = true;
                    } else if first.parse::<u32>().is_ok() {
                        options.routing = "account".into();
                        options.account = Some(options.positionals.remove(0));
                        worker_dispatch |= matches!(launcher, Launcher::Universal) && !options.solo;
                    } else {
                        options.routing = "default".into();
                    }
                } else {
                    options.routing = "default".into();
                }
            } else {
                options.routing = "default".into();
            }
        }
        if worker_dispatch
            && !attachment_explicit
            && !matches!(launcher, Launcher::AccountHelper(_))
        {
            // A fixed run ID is the scheduler's inline worker mode. Detached
            // execution remains available when the caller explicitly asks for
            // it, while generated worker IDs retain the detached default.
            let fixed_run_id = options.run_id.is_some();
            options.detach = !fixed_run_id;
            options.foreground = fixed_run_id;
        }
        if options.solo {
            if matches!(launcher, Launcher::AccountHelper(_)) {
                bail!("--solo is not valid for account helpers");
            }
            if let Some(flag) = solo_forbidden_flag {
                bail!("solo mode does not support {flag}");
            }
            if worker_dispatch {
                bail!("--solo cannot be combined with worker dispatch");
            }
            options.foreground = true;
            options.detach = false;
        }
        if let Some(engine) = options.engine.or(match launcher {
            Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => {
                Some(engine)
            }
            Launcher::Universal => None,
        }) {
            normalize_provider_options(engine, &mut options)?;
        }
        validation::normalize_goal(&mut options.goal)?;
        if options.max_turns.is_some() && options.goal.is_none() {
            bail!("--max-turns requires --goal");
        }
        options.worker_dispatch = worker_dispatch;
        validation::validate_solo_options(&options)?;
        super::model_validation::validate(launcher, &options)?;
        validation::validate_worker_task(options.worker_dispatch, &options.positionals)?;
        validation::validate_run_metadata(&options)?;
        validation::validate_effective_base(&options)?;
        Ok(options)
    }
}

fn mark_solo_forbidden(slot: &mut Option<String>, flag: &str) {
    if slot.is_none() {
        *slot = Some(flag.to_owned());
    }
}

pub(crate) fn normalize_provider_options(
    engine: Engine,
    options: &mut LaunchOptions,
) -> Result<()> {
    if options.opus {
        if engine != Engine::Claude {
            bail!("--opus is only valid with --engine claude");
        }
        if let Some(model) = options
            .provider_models
            .get(&Engine::Claude)
            .map(String::as_str)
            .or(options.model.as_deref())
        {
            if !model
                .split_once('[')
                .map_or(model, |(base, _)| base)
                .eq_ignore_ascii_case("claude-opus-5")
            {
                bail!("--opus conflicts with the explicit Claude model {model:?}");
            }
        }
    }

    if matches!(engine, Engine::Opencode | Engine::Kimi | Engine::Grok)
        && (options.opus || options.ultra || options.effort.is_some())
    {
        bail!("--opus/-u/-e do not apply to {} workers", engine.as_str());
    }

    if engine == Engine::Codex {
        if let Some(effort) = options.effort.as_deref() {
            if !matches!(effort, "low" | "medium" | "high" | "xhigh") {
                bail!("Codex effort must be low, medium, high, or xhigh");
            }
        }
        if options.ultra {
            options.effort = Some(options.effort.clone().unwrap_or_else(|| "xhigh".into()));
            options.ultra = false;
        }
    } else if engine == Engine::Claude && options.ultra && options.effort.is_none() {
        options.effort = Some("xhigh".into());
    }
    Ok(())
}

pub(crate) fn join_positionals(positionals: Vec<String>) -> Option<String> {
    let task = positionals.join(" ").trim().to_owned();
    (!task.is_empty()).then_some(task)
}

fn split_flag(value: &str) -> (&str, Option<&str>) {
    value
        .split_once('=')
        .map_or((value, None), |(flag, value)| (flag, Some(value)))
}

fn value(args: &[String], index: &mut usize, flag: &str, inline: Option<&str>) -> Result<String> {
    if let Some(inline) = inline {
        if inline.is_empty() {
            bail!("{flag} requires a value");
        }
        return Ok(inline.to_owned());
    }
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

fn value_allow_empty(
    args: &[String],
    index: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String> {
    if let Some(inline) = inline {
        return Ok(inline.to_owned());
    }
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

fn minutes(args: &[String], index: &mut usize, flag: &str, inline: Option<&str>) -> Result<f64> {
    let raw = value(args, index, flag, inline)?;
    let minutes = raw.parse::<f64>().map_err(|_| {
        anyhow::anyhow!("{flag} requires a number of minutes between 0 and {MAX_TIMEOUT_MINUTES}")
    })?;
    if !minutes.is_finite() || !(0.0..=MAX_TIMEOUT_MINUTES).contains(&minutes) {
        bail!("{flag} requires a number of minutes between 0 and {MAX_TIMEOUT_MINUTES}");
    }
    Ok(minutes)
}

fn engine_model_flag(flag: &str) -> Option<Engine> {
    match flag {
        "--claude-model" => Some(Engine::Claude),
        "--codex-model" | "-cm" => Some(Engine::Codex),
        "--opencode-model" => Some(Engine::Opencode),
        "--kimi-model" => Some(Engine::Kimi),
        "--grok-model" => Some(Engine::Grok),
        _ => None,
    }
}
