use anyhow::{Result, bail};
use neomax_core::Engine;

use crate::parser;

pub(crate) fn run_id(args: &[String]) -> Result<String> {
    let values = positional(args, &["--json", "--patch", "--log", "--any"])?;
    match values.as_slice() {
        [id] if valid_run_id(id) => Ok(id.clone()),
        [] => bail!("a run id is required"),
        [_] => bail!("run id contains unsafe path characters"),
        _ => bail!("exactly one run id is required"),
    }
}

pub(crate) fn valid_run_id(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && value.chars().count() <= 128
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(crate) fn positional(args: &[String], flags: &[&str]) -> Result<Vec<String>> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        let name = value
            .split_once('=')
            .map_or(value.as_str(), |(name, _)| name);
        if matches!(name, "--engine" | "--status" | "--limit") {
            if !value.contains('=') {
                if index + 1 >= args.len() {
                    bail!("{value} requires a value");
                }
                index += 1;
            } else if value.ends_with('=') {
                bail!("{value} requires a value");
            }
            index += 1;
            continue;
        }
        if flags.contains(&name) {
            if value != name {
                bail!("{name} does not take a value");
            }
            index += 1;
            continue;
        }
        if value.starts_with('-') {
            bail!("unknown option {value}");
        }
        values.push(value.clone());
        index += 1;
    }
    Ok(values)
}

pub(crate) fn engine(args: &[String]) -> Result<Option<Engine>> {
    parser::value(args, "--engine")?
        .map(|value| value.parse().map_err(anyhow::Error::msg))
        .transpose()
}

pub(crate) fn status(args: &[String]) -> Result<Option<String>> {
    parser::value(args, "--status")
}

pub(crate) fn limit(args: &[String], fallback: usize) -> Result<usize> {
    let Some(value) = parser::value(args, "--limit")? else {
        return Ok(fallback);
    };
    let limit = value
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("--limit must be a non-negative integer"))?;
    if limit > 10_000 {
        bail!("--limit cannot exceed 10000");
    }
    Ok(limit)
}

pub(crate) fn retry_selector(args: &[String]) -> Result<Option<String>> {
    let values = positional(args, &["--json", "--any"])?;
    Ok(values.into_iter().nth(1))
}

pub(crate) fn json(args: &[String]) -> bool {
    parser::has(args, "--json")
}

pub(crate) fn patch(args: &[String]) -> bool {
    parser::has(args, "--patch")
}
