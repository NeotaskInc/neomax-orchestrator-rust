use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;

use crate::models;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AuthOptions {
    pub(crate) engine: Option<Engine>,
    pub(crate) destination: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) restore: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) swap: bool,
    pub(crate) log: bool,
    pub(crate) json: bool,
}

impl AuthOptions {
    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            let current = &args[index];
            let (flag, inline) = current
                .split_once('=')
                .map_or((current.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            match flag {
                "--json" => options.json = true,
                "--swap" => options.swap = true,
                "--log" => options.log = true,
                "--engine" => {
                    options.engine = Some(models::parse_engine(&option_value(
                        args, &mut index, flag, inline,
                    )?)?);
                }
                "--from" | "--source" => {
                    options.source = Some(option_value(args, &mut index, flag, inline)?);
                }
                "--to" | "--destination" => {
                    options.destination = Some(option_value(args, &mut index, flag, inline)?);
                }
                "--restore" => {
                    options.restore = Some(option_value(args, &mut index, flag, inline)?);
                }
                "--reason" => {
                    options.reason = Some(option_value(args, &mut index, flag, inline)?);
                }
                value if value.starts_with('-') => bail!("rotate-auth: unknown option {current}"),
                value => {
                    if options.destination.replace(value.to_owned()).is_some() {
                        bail!("rotate-auth accepts only one destination profile");
                    }
                }
            }
            index += 1;
        }
        if options.swap && options.restore.is_some() {
            bail!("rotate-auth: --swap cannot be combined with --restore");
        }
        if options.restore.is_some() && (options.destination.is_some() || options.source.is_some())
        {
            bail!("rotate-auth: --restore cannot be combined with copy or swap arguments");
        }
        if options.log
            && (options.swap
                || options.source.is_some()
                || options.destination.is_some()
                || options.restore.is_some())
        {
            bail!("rotate-auth: --log cannot be combined with a mutation");
        }
        Ok(options)
    }
}

fn option_value(
    args: &[String],
    index: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String> {
    if let Some(value) = inline {
        if value.is_empty() {
            bail!("{flag} requires a value");
        }
        return Ok(value.to_owned());
    }
    let value = args
        .get(*index + 1)
        .with_context(|| format!("{flag} requires a value"))?;
    *index += 1;
    Ok(value.clone())
}

pub(crate) fn selectors_need_discovery(options: &AuthOptions) -> bool {
    [
        options.destination.as_deref(),
        options.source.as_deref(),
        options.restore.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|selector| {
        let path = std::path::Path::new(selector);
        !path.is_absolute()
            && !selector.contains('/')
            && !selector.contains('\\')
            && !selector.starts_with('.')
    })
}

pub(crate) fn launcher_engine(launcher: Launcher) -> Option<Engine> {
    match launcher {
        Launcher::ProviderOrchestrator(engine) | Launcher::AccountHelper(engine) => Some(engine),
        Launcher::Universal => None,
    }
}
