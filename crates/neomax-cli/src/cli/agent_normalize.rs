use neomax_core::agent_tools::resolve_agent_command;

pub(super) fn normalize_command_args(args: &[String]) -> Option<Vec<String>> {
    let command = resolve_agent_command(args).ok()?;
    if command == "help" {
        return None;
    }
    let command_parts = command.split_whitespace().collect::<Vec<_>>();
    let command_indices = command_positionals(args, &command_parts)?;
    let command_prefix_len = command_indices.len();
    if command_prefix_len == 0 {
        return None;
    }
    let mut normalized = command_parts
        .iter()
        .take(command_prefix_len)
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    let discard_prefix =
        command == "portal" && command_indices.first().is_some_and(|index| *index > 0);
    for (index, argument) in args.iter().enumerate() {
        if !command_indices.contains(&index)
            && !(discard_prefix
                && command_indices
                    .first()
                    .is_some_and(|command_index| index < *command_index))
        {
            normalized.push(argument.clone());
        }
    }
    (normalized != args).then_some(normalized)
}

fn command_positionals(args: &[String], command_parts: &[&str]) -> Option<Vec<usize>> {
    let mut positionals = Vec::new();
    let mut after_separator = false;
    let mut skip_next = false;
    for (index, argument) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if after_separator {
            positionals.push((index, argument.as_str()));
            continue;
        }
        if argument == "--" {
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
        positionals.push((index, argument.as_str()));
    }
    if positionals.is_empty() {
        return None;
    }
    let matched = command_parts
        .iter()
        .zip(positionals.iter())
        .take_while(|(expected, (_, actual))| **expected == *actual)
        .map(|(_, (index, _))| *index)
        .collect::<Vec<_>>();
    (!matched.is_empty()).then_some(matched)
}

fn agent_flag_takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "--engine"
            | "--workers"
            | "--model"
            | "--claude-model"
            | "--codex-model"
            | "-cm"
            | "--opencode-model"
            | "--kimi-model"
            | "--grok-model"
            | "--goal"
            | "--base"
            | "--run-id"
            | "--tag"
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
    use super::normalize_command_args;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn moves_canonical_commands_ahead_of_global_options() {
        assert_eq!(
            normalize_command_args(&args(&["--dry-run", "--engine", "opencode", "status",])),
            Some(args(&["status", "--dry-run", "--engine", "opencode"]))
        );
        assert_eq!(
            normalize_command_args(&args(&["config", "--json", "set", "max-subagents", "4",])),
            Some(args(&["config", "set", "--json", "max-subagents", "4"]))
        );
    }

    #[test]
    fn portal_drops_launcher_options_before_the_command() {
        let actual = normalize_command_args(&args(&[
            "--engine",
            "opencode",
            "--workers",
            "all",
            "--model",
            "opencode/big-pickle",
            "portal",
            "--port",
            "4317",
        ]));
        assert_eq!(
            actual,
            Some(args(&["portal", "--port", "4317"])),
            "launcher options must not be forwarded to neomax-portal"
        );
    }

    #[test]
    fn status_keeps_its_supported_filters_when_options_precede_the_command() {
        let actual =
            normalize_command_args(&args(&["--workers", "all", "status", "--engine", "codex"]));
        assert_eq!(
            actual,
            Some(args(&["status", "--workers", "all", "--engine", "codex"]))
        );
    }

    #[test]
    fn leaves_explicit_worker_dispatch_and_separator_tasks_unchanged() {
        assert_eq!(
            normalize_command_args(&args(&["--worker-dispatch", "fixture task"])),
            None
        );
        assert_eq!(
            normalize_command_args(&args(&["dispatch", "--", "fixture task"])),
            None
        );
        assert_eq!(normalize_command_args(&args(&["--", "status"])), None);
    }
}
