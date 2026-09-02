use std::env;
use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::address::LocalBind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalArgs {
    pub bind: LocalBind,
    pub home: Option<PathBuf>,
    pub state: Option<PathBuf>,
    pub days: u32,
    pub max_artifact_bytes: usize,
}

impl Default for PortalArgs {
    fn default() -> Self {
        Self {
            bind: LocalBind::default(),
            home: None,
            state: None,
            days: 30,
            max_artifact_bytes: 128 * 1024 * 1024,
        }
    }
}

impl PortalArgs {
    pub fn parse_env() -> Result<Self> {
        Self::parse(env::args().skip(1))
    }

    pub fn parse<I, S>(arguments: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = Self::default();
        let mut positional_port = None;
        let mut iterator = arguments.into_iter().map(Into::into);
        while let Some(argument) = iterator.next() {
            match argument.as_str() {
                "-h" | "--help" => bail!(usage()),
                "--bind" => args.bind = next_value(&mut iterator, "--bind")?.parse()?,
                "--port" => {
                    let port = next_value(&mut iterator, "--port")?.parse::<u16>()?;
                    if port == 0 {
                        bail!("--port must be between 1 and 65535");
                    }
                    args.bind = LocalBind::loopback(port);
                }
                "--home" => args.home = Some(PathBuf::from(next_value(&mut iterator, "--home")?)),
                "--state" | "--neomax-home" => {
                    args.state = Some(PathBuf::from(next_value(&mut iterator, &argument)?))
                }
                "--days" => args.days = parse_days(&next_value(&mut iterator, "--days")?)?,
                "--max-artifact-bytes" => {
                    args.max_artifact_bytes =
                        next_value(&mut iterator, "--max-artifact-bytes")?.parse()?;
                    if args.max_artifact_bytes == 0 {
                        bail!("--max-artifact-bytes must be positive");
                    }
                }
                value if value.starts_with('-') => bail!("unknown portal option: {value}"),
                value => {
                    if positional_port.is_some() {
                        bail!("unexpected portal argument: {value}");
                    }
                    positional_port = Some(value.parse::<u16>()?);
                }
            }
        }
        if let Some(port) = positional_port {
            if port == 0 {
                bail!("portal port must be between 1 and 65535");
            }
            args.bind = LocalBind::loopback(port);
        }
        Ok(args)
    }
}

fn next_value<I>(iterator: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    iterator
        .next()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))
}

fn parse_days(value: &str) -> Result<u32> {
    let days = value.parse::<u32>()?;
    if days > 3660 {
        bail!("--days must be between 0 and 3660");
    }
    Ok(days)
}

pub fn usage() -> &'static str {
    "usage: neomax-portal [PORT] [--bind LOOPBACK:PORT] [--home PATH] [--state PATH] [--days N] [--max-artifact-bytes N]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reference_port_and_relocated_state() {
        let args = PortalArgs::parse([
            "9000",
            "--home",
            "/fixture/home",
            "--state",
            "/fixture/state",
            "--days",
            "7",
        ])
        .unwrap();
        assert_eq!(args.bind.port(), 9000);
        assert_eq!(args.home, Some(PathBuf::from("/fixture/home")));
        assert_eq!(args.state, Some(PathBuf::from("/fixture/state")));
        assert_eq!(args.days, 7);
    }

    #[test]
    fn refuses_unknown_options_and_unbounded_windows() {
        assert!(PortalArgs::parse(["--wat"]).is_err());
        assert!(PortalArgs::parse(["--days", "3661"]).is_err());
        assert!(PortalArgs::parse(["0"]).is_err());
    }

    #[test]
    fn help_lists_every_runtime_limit() {
        assert!(usage().contains("--max-artifact-bytes N"));
    }
}
