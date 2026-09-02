use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ParsedArgs {
    pub flags: BTreeMap<String, String>,
    pub switches: BTreeSet<String>,
    pub positionals: Vec<String>,
}

impl ParsedArgs {
    pub fn value(&self, flag: &str) -> Option<&str> {
        self.flags.get(flag).map(String::as_str)
    }

    pub fn has(&self, flag: &str) -> bool {
        self.switches.contains(flag)
    }

    pub fn positional(&self, index: usize, label: &str) -> Result<&str> {
        self.positionals
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| anyhow::anyhow!("{label} requires an argument"))
    }
}

pub(super) fn parse(
    args: &[String],
    value_flags: &[&str],
    switch_flags: &[&str],
) -> Result<ParsedArgs> {
    let values = value_flags.iter().copied().collect::<BTreeSet<_>>();
    let switches = switch_flags.iter().copied().collect::<BTreeSet<_>>();
    let mut parsed = ParsedArgs::default();
    let mut index = 0;
    while index < args.len() {
        let current = &args[index];
        if !current.starts_with('-') {
            parsed.positionals.push(current.clone());
            index += 1;
            continue;
        }
        let (name, inline) = current
            .split_once('=')
            .map_or((current.as_str(), None), |(name, value)| {
                (name, Some(value))
            });
        if switches.contains(name) {
            if inline.is_some() {
                bail!("{name} does not accept a value");
            }
            parsed.switches.insert(name.to_owned());
            index += 1;
            continue;
        }
        if !values.contains(name) {
            bail!("unknown option {name}");
        }
        let value = match inline {
            Some(value) if !value.is_empty() => value.to_owned(),
            Some(_) => bail!("{name} requires a value"),
            None => {
                let Some(value) = args.get(index + 1) else {
                    bail!("{name} requires a value");
                };
                if value.starts_with('-') {
                    bail!("{name} requires a value");
                }
                index += 1;
                value.clone()
            }
        };
        parsed.flags.insert(name.to_owned(), value);
        index += 1;
    }
    Ok(parsed)
}

pub(super) fn positive(value: &str, label: &str) -> Result<usize> {
    let value = value
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("{label} must be a positive integer"))?;
    if value == 0 {
        bail!("{label} must be a positive integer");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_values_switches_and_positionals_without_guessing() {
        let args = [
            "--project=demo".to_owned(),
            "issue-1".to_owned(),
            "--json".to_owned(),
            "--body".to_owned(),
            "text".to_owned(),
        ];
        let parsed = parse(&args, &["--project", "--body"], &["--json"]).unwrap();
        assert_eq!(parsed.value("--project"), Some("demo"));
        assert_eq!(parsed.value("--body"), Some("text"));
        assert!(parsed.has("--json"));
        assert_eq!(parsed.positionals, vec!["issue-1"]);
    }

    #[test]
    fn rejects_unknown_and_missing_values() {
        assert!(parse(&["--nope".into()], &[], &[]).is_err());
        assert!(parse(&["--project".into()], &["--project"], &[]).is_err());
        assert!(parse(&["--json=true".into()], &[], &["--json"]).is_err());
    }
}
