use std::ffi::OsString;

use anyhow::{Context, Result, bail};

pub fn utf8_args(values: Vec<OsString>) -> Result<Vec<String>> {
    values
        .into_iter()
        .map(|value| {
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("arguments must be valid UTF-8"))
        })
        .collect()
}

pub fn value(args: &[String], flag: &str) -> Result<Option<String>> {
    let mut result = None;
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        if current == flag {
            let next = args
                .get(index + 1)
                .with_context(|| format!("{flag} requires a value"))?;
            result = Some(next.clone());
            index += 2;
            continue;
        }
        if let Some(inline) = current.strip_prefix(&format!("{flag}=")) {
            if inline.is_empty() {
                bail!("{flag} requires a value");
            }
            result = Some(inline.to_owned());
        }
        index += 1;
    }
    Ok(result)
}

pub fn has(args: &[String], flag: &str) -> bool {
    args.iter().any(|value| value == flag)
}

pub fn positional(args: &[String], value_flags: &[&str]) -> Result<Vec<String>> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        let flag = current.split('=').next().unwrap_or(current);
        let is_value_flag = current
            .split('=')
            .next()
            .is_some_and(|flag| value_flags.contains(&flag));
        if is_value_flag {
            if !current.contains('=') {
                index += 1;
                if index >= args.len() {
                    bail!("{flag} requires a value");
                }
            }
        } else if current.starts_with('-') {
            bail!("unknown option {current}");
        } else {
            output.push(current.clone());
        }
        index += 1;
    }
    Ok(output)
}

pub fn parse_u32(value: &str, label: &str) -> Result<u32> {
    value
        .parse::<u32>()
        .with_context(|| format!("{label} must be a non-negative integer"))
}

pub fn parse_positive_u32(value: &str, label: &str) -> Result<u32> {
    let parsed = parse_u32(value, label)?;
    if parsed == 0 {
        bail!("{label} must be a positive integer");
    }
    Ok(parsed)
}
