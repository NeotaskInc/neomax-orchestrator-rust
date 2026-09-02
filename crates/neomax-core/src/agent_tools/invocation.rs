use crate::{Error, Result};

use super::commands::CANONICAL_COMMANDS;

/// Resolve an agent invocation to the exact command name authorized by the
/// canonical manifest. Agent calls do not have the human launcher's free-form
/// task fallback because that fallback would turn a malformed tool call into
/// an unguarded root provider launch.
pub fn resolve_agent_command(args: &[String]) -> Result<&'static str> {
    if has_pre_separator_flag(args, |argument| {
        matches!(argument, "-h" | "--help" | "--version" | "-V")
    }) {
        return Ok("help");
    }

    if worker_dispatch_before_command(args) {
        return Ok("dispatch");
    }

    let positionals = agent_positionals(args);
    let Some(first) = positionals.first().copied() else {
        return Err(missing_command());
    };

    if first == "commands" {
        return Ok("help");
    }

    if let Some(second) = positionals.get(1).copied() {
        let mut pair = String::with_capacity(first.len() + second.len() + 1);
        pair.push_str(first);
        pair.push(' ');
        pair.push_str(second);
        if let Some(command) = canonical_command(&pair) {
            return Ok(command);
        }
    }

    if first == "config" && positionals.get(1).is_none() {
        return Ok(canonical_command("config show").expect("canonical config show command"));
    }

    if first == "config" {
        return Err(Error::InvalidArgument(format!(
            "unknown config operation {:?} for agent invocation; use `config show` or `config set`",
            positionals.get(1).copied().unwrap_or_default()
        )));
    }
    if first == "account" {
        return Err(Error::InvalidArgument(
            "account requires a canonical operation for agent calls; use `account status`, `account pause`, `account unpause`, or `account rotate`".into(),
        ));
    }

    if let Some(command) = canonical_command(first) {
        return Ok(command);
    }

    Err(unknown_command(first))
}

fn canonical_command(name: &str) -> Option<&'static str> {
    CANONICAL_COMMANDS
        .iter()
        .find(|command| command.command == name)
        .map(|command| command.command)
}

fn missing_command() -> Error {
    Error::InvalidArgument(
        "agent invocation requires a canonical Neomax tool command; use `dispatch TASK` for worker work or inspect NEOMAX_TOOL_MANIFEST".into(),
    )
}

fn unknown_command(command: &str) -> Error {
    Error::InvalidArgument(format!(
        "unknown agent command {:?}; use a canonical command from NEOMAX_TOOL_MANIFEST (for worker work, use `dispatch TASK`)",
        command
    ))
}

fn agent_positionals(args: &[String]) -> Vec<&str> {
    let mut positionals = Vec::new();
    let mut after_separator = false;
    let mut skip_next = false;
    for argument in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if after_separator {
            positionals.push(argument.as_str());
            continue;
        }
        if argument == "--" {
            // A separator before the first command turns every following
            // token into task text. It must not make a free-form token look
            // like an authorized tool command.
            if positionals.is_empty() {
                break;
            }
            after_separator = true;
            continue;
        }
        if argument.starts_with('-') {
            let (flag, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(flag, value)| {
                    (flag, Some(value))
                });
            if inline.is_none() && agent_flag_takes_value(flag) {
                skip_next = true;
            }
            continue;
        }
        positionals.push(argument.as_str());
    }
    positionals
}

fn has_pre_separator_flag<F>(args: &[String], predicate: F) -> bool
where
    F: Fn(&str) -> bool,
{
    for argument in args {
        if argument == "--" {
            return false;
        }
        if predicate(argument) {
            return true;
        }
    }
    false
}

fn worker_dispatch_before_command(args: &[String]) -> bool {
    let mut marker = None;
    let mut first_positional = None;
    let mut skip_next = false;
    for (index, argument) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if argument == "--" {
            break;
        }
        if argument == "--worker-dispatch" {
            marker = Some(index);
            continue;
        }
        if argument.starts_with('-') {
            let (flag, inline) = argument
                .split_once('=')
                .map_or((argument.as_str(), None), |(flag, value)| {
                    (flag, Some(value))
                });
            if inline.is_none() && agent_flag_takes_value(flag) {
                skip_next = true;
            }
            continue;
        }
        first_positional.get_or_insert(index);
    }
    marker.is_some_and(|marker| {
        first_positional.is_none_or(|first_positional| marker < first_positional)
    })
}

fn agent_flag_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--engine"
            | "--workers"
            | "--model"
            | "--claude-model"
            | "--codex-model"
            | "--opencode-model"
            | "--kimi-model"
            | "--grok-model"
            | "--goal"
            | "--base"
            | "--session-id"
            | "--max-turns"
            | "--prefer"
            | "--priority"
            | "--account"
            | "--cwd"
            | "--dir"
            | "--effort"
            | "-e"
            | "-t"
            | "-s"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn resolves_every_canonical_manifest_command() {
        for command in CANONICAL_COMMANDS {
            let arguments = command
                .command
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            assert_eq!(
                resolve_agent_command(&arguments).unwrap(),
                command.command,
                "manifest command is not recognized: {}",
                command.command
            );
        }
    }

    #[test]
    fn skips_global_options_without_accepting_free_form_root_tasks() {
        assert_eq!(resolve_agent_command(&args(&["commands"])).unwrap(), "help");
        assert_eq!(
            resolve_agent_command(&args(&["--worker-dispatch", "fixture task"])).unwrap(),
            "dispatch"
        );
        assert_eq!(
            resolve_agent_command(&args(&["--dry-run", "--engine", "opencode", "status"])).unwrap(),
            "status"
        );
        assert_eq!(
            resolve_agent_command(&args(&["config", "--json"])).unwrap(),
            "config show"
        );
        assert_eq!(
            resolve_agent_command(&args(&["dispatch", "--", "fix", "the", "build"])).unwrap(),
            "dispatch"
        );
    }

    #[test]
    fn rejects_empty_unknown_and_retired_alias_invocations() {
        for arguments in [
            args(&[]),
            args(&["fix", "the", "build"]),
            args(&["auto", "fix", "the", "build"]),
            args(&["delegate", "fix", "the", "build"]),
        ] {
            let error = resolve_agent_command(&arguments).unwrap_err().to_string();
            assert!(
                error.contains("agent invocation") || error.contains("agent command"),
                "error was not actionable: {error}"
            );
        }
    }

    #[test]
    fn rejects_unknown_nested_operations() {
        let config_error = resolve_agent_command(&args(&["config", "bogus"]))
            .unwrap_err()
            .to_string();
        assert!(config_error.contains("config operation"));
        let account_error = resolve_agent_command(&args(&["account", "bogus"]))
            .unwrap_err()
            .to_string();
        assert!(account_error.contains("account requires"));
    }

    #[test]
    fn separators_cannot_turn_task_text_into_an_authorized_command() {
        for arguments in [
            args(&["--", "status"]),
            args(&["--", "dispatch", "task"]),
            args(&["--", "--worker-dispatch", "task"]),
            args(&["fix", "--worker-dispatch", "task"]),
        ] {
            assert!(
                resolve_agent_command(&arguments).is_err(),
                "separator unexpectedly authorized an invocation: {arguments:?}"
            );
        }
        assert_eq!(
            resolve_agent_command(&args(&["dispatch", "--", "task"])).unwrap(),
            "dispatch"
        );
    }
}
