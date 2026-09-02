use anyhow::{Result, bail};
use neomax_core::WorkerScope;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RotationOptions {
    pub(crate) ids: Vec<String>,
    pub(crate) scope: Option<WorkerScope>,
    pub(crate) active: bool,
}

impl RotationOptions {
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
                "--json" | "--all" | "--dry-run" => {}
                "--active" => options.active = true,
                "--workers" => {
                    let value = option_value(args, &mut index, flag, inline)?;
                    options.scope = Some(value.parse()?);
                }
                "--engine" => {
                    let value = option_value(args, &mut index, flag, inline)?;
                    options.scope = Some(WorkerScope::only(value.parse()?));
                }
                "--run" => {
                    options
                        .ids
                        .push(option_value(args, &mut index, flag, inline)?);
                }
                value if value.starts_with('-') => bail!("unknown rotation option {current}"),
                value => options.ids.push(value.to_owned()),
            }
            index += 1;
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
        .ok_or_else(|| anyhow::anyhow!("{flag} requires a value"))?;
    *index += 1;
    Ok(value.clone())
}
