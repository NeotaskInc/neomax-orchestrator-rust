use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedLaunch {
    pub args: Vec<String>,
    pub resume: bool,
}

pub(crate) fn normalize(args: &[String]) -> Result<Option<NormalizedLaunch>> {
    let mut marker = None;
    let mut scan = 0;
    while scan < args.len() {
        let arg = &args[scan];
        if arg == "--" {
            break;
        }
        if is_value_flag(arg) {
            scan += if arg.contains('=') { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            scan += 1;
            continue;
        }
        if matches!(arg.as_str(), "launch" | "session")
            && args.get(scan + 1).map(String::as_str) == Some("resume")
        {
            marker = Some(scan);
        }
        break;
    }
    let Some(marker) = marker else {
        return Ok(None);
    };

    let tail = &args[marker + 2..];
    let mut normalized = args[..marker].to_vec();
    let resume_index = normalized.len();
    normalized.push("--resume".into());
    normalized.reserve(tail.len() + 2);
    let mut selector = None;
    let mut explicit_session = false;
    let mut index = 0;
    let mut after_separator = false;
    while index < tail.len() {
        let arg = &tail[index];
        if !after_separator && arg == "--" {
            after_separator = true;
            normalized.push(arg.clone());
            index += 1;
            continue;
        }
        if !after_separator && is_value_flag(arg) {
            if arg == "--session-id" || arg.starts_with("--session-id=") {
                explicit_session = true;
            }
            normalized.push(arg.clone());
            if !arg.contains('=') {
                let value = tail.get(index + 1).ok_or_else(|| {
                    anyhow::anyhow!("{arg} requires a value in nested resume syntax")
                })?;
                if arg == "--session-id" && value.trim().is_empty() {
                    bail!("--session-id requires a non-empty value");
                }
                normalized.push(value.clone());
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if !after_separator && arg == "--resume" {
            index += 1;
            continue;
        }
        if !after_separator && arg.starts_with('-') {
            normalized.push(arg.clone());
            index += 1;
            continue;
        }
        // Once an explicit session is present, remaining positionals are the
        // follow-up prompt. A positional seen first remains a selector and
        // conflicts with the later explicit session.
        if selector.is_none() && !explicit_session && !after_separator {
            selector = Some(arg.clone());
        } else {
            normalized.push(arg.clone());
        }
        index += 1;
    }

    if let Some(selector) = selector {
        if explicit_session {
            bail!("nested resume received both a positional session selector and --session-id");
        }
        if selector.eq_ignore_ascii_case("resume") {
            bail!("nested resume syntax contains a second resume marker");
        }
        normalized.splice(
            resume_index + 1..resume_index + 1,
            ["--session-id".into(), selector],
        );
    }
    Ok(Some(NormalizedLaunch {
        args: normalized,
        resume: true,
    }))
}

fn is_value_flag(arg: &str) -> bool {
    let flag = arg.split_once('=').map_or(arg, |(flag, _)| flag);
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
            | "-e"
            | "-t"
            | "-s"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_resume_normalizes_selector_before_provider_options() {
        let parsed = normalize(&[
            "launch".into(),
            "resume".into(),
            "session-1".into(),
            "--engine".into(),
            "kimi".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed.args,
            ["--resume", "--session-id", "session-1", "--engine", "kimi"]
        );
        assert!(parsed.resume);
    }

    #[test]
    fn nested_resume_keeps_explicit_session_id_and_prompt() {
        let parsed = normalize(&[
            "session".into(),
            "resume".into(),
            "--session-id".into(),
            "session-1".into(),
            "--engine".into(),
            "kimi".into(),
            "follow-up".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed.args,
            [
                "--resume",
                "--session-id",
                "session-1",
                "--engine",
                "kimi",
                "follow-up"
            ]
        );
    }

    #[test]
    fn nested_resume_accepts_launch_flags_before_the_command_marker() {
        let parsed = normalize(&[
            "--json".into(),
            "--foreground".into(),
            "launch".into(),
            "resume".into(),
            "session-1".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(
            parsed.args,
            [
                "--json",
                "--foreground",
                "--resume",
                "--session-id",
                "session-1"
            ]
        );
    }

    #[test]
    fn selector_parser_does_not_consume_option_values() {
        let parsed = normalize(&[
            "launch".into(),
            "resume".into(),
            "--engine".into(),
            "kimi".into(),
            "session-1".into(),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.args[0], "--resume");
        assert_eq!(parsed.args[2], "session-1");
        assert_eq!(parsed.args[4], "kimi");
    }

    #[test]
    fn positional_selector_before_explicit_session_fails_closed() {
        let error = normalize(&[
            "session".into(),
            "resume".into(),
            "session-1".into(),
            "--session-id".into(),
            "session-2".into(),
        ])
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("both a positional session selector and --session-id")
        );
    }
}
