use std::fmt;

use anyhow::{Result, bail};
use neomax_core::usage::{PriceCatalog, UsageLedger, UsageReport, build_usage_report};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::error;
use crate::parser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UsageRange {
    Days(u32),
    Since { seconds: i64 },
    All,
}

impl UsageRange {
    pub(crate) fn parse(args: &[String]) -> Result<Self> {
        validate_args(args)?;
        let days = parser::value(args, "--days")?;
        let since = parser::value(args, "--since")?;
        let all = parser::has(args, "--all");
        let selectors =
            usize::from(days.is_some()) + usize::from(since.is_some()) + usize::from(all);
        if selectors > 1 {
            bail!("usage accepts only one of --days, --since, or --all");
        }
        if let Some(value) = days {
            let days = value
                .parse::<u32>()
                .map_err(|_| anyhow::anyhow!("usage: --days must be an integer"))?;
            if days == 0 {
                bail!("usage: --days must be greater than zero");
            }
            return Ok(Self::Days(days));
        }
        if let Some(value) = since {
            let seconds = parse_duration(&value)?;
            if seconds <= 0 {
                bail!("usage: --since must be greater than zero");
            }
            return Ok(Self::Since { seconds });
        }
        if all {
            Ok(Self::All)
        } else {
            Ok(Self::Days(30))
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Self::Days(days) => format!("{days} days"),
            Self::Since { seconds } => format!("since {}", format_duration(*seconds)),
            Self::All => "all history".to_owned(),
        }
    }

    fn report_days(&self) -> u32 {
        match self {
            Self::Days(days) => *days,
            Self::Since { seconds } => seconds
                .saturating_add(86_399)
                .div_euclid(86_400)
                .min(i64::from(u32::MAX)) as u32,
            Self::All => 0,
        }
    }

    fn records(
        &self,
        ledger: &UsageLedger,
        now: i64,
    ) -> Result<Vec<neomax_core::usage::LedgerRecord>> {
        match self {
            Self::Days(days) => Ok(ledger
                .read_deduplicated(*days, now)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?),
            Self::Since { seconds } => Ok(ledger
                .read_deduplicated_since(now.saturating_sub(*seconds))
                .map_err(|error| anyhow::anyhow!(error.to_string()))?),
            Self::All => Ok(ledger
                .read_deduplicated_since(0)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?),
        }
    }
}

fn validate_args(args: &[String]) -> Result<()> {
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        if value == "--json" || value == "--all" {
            index += 1;
            continue;
        }
        let flag = value.split('=').next().unwrap_or(value);
        if matches!(flag, "--days" | "--since") {
            if !value.contains('=') {
                if args.get(index + 1).is_none() {
                    bail!("{flag} requires a value");
                }
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if value.starts_with('-') {
            bail!("usage: unknown option {value}");
        }
        bail!("usage: unexpected argument {value}");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct UsageOutput {
    #[serde(flatten)]
    pub report: UsageReport,
    pub range: String,
}

pub(crate) fn collect(
    context: &RuntimeContext,
    args: &[String],
) -> Result<(UsageOutput, UsageRange)> {
    let range = error::usage(UsageRange::parse(args))?;
    let ledger = UsageLedger::new(&context.paths.usage_ledger);
    let records = range.records(&ledger, context.now)?;
    let report = build_usage_report(
        &records,
        range.report_days(),
        context.now,
        &PriceCatalog::default(),
    );
    let output = UsageOutput {
        report,
        range: range.label(),
    };
    Ok((output, range))
}

pub(super) fn parse_duration(value: &str) -> Result<i64> {
    let value = value.trim().to_ascii_lowercase();
    let (number, suffix) = value.split_at(
        value
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(value.len()),
    );
    if number.is_empty() || suffix.is_empty() {
        bail!("usage: --since must look like 90s, 37m, 2h, 3d, or 1w");
    }
    let amount = number
        .parse::<i64>()
        .map_err(|_| anyhow::anyhow!("usage: --since has an invalid amount"))?;
    let multiplier = match suffix {
        "s" => 1,
        "m" => 60,
        "h" => 3_600,
        "d" => 86_400,
        "w" => 604_800,
        _ => bail!("usage: --since must end in s, m, h, d, or w"),
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(|| anyhow::anyhow!("usage: --since is too large"))
}

fn format_duration(seconds: i64) -> String {
    if seconds % 604_800 == 0 {
        return format!("{}w", seconds / 604_800);
    }
    if seconds % 86_400 == 0 {
        return format!("{}d", seconds / 86_400);
    }
    if seconds % 3_600 == 0 {
        return format!("{}h", seconds / 3_600);
    }
    if seconds % 60 == 0 {
        return format!("{}m", seconds / 60);
    }
    format!("{seconds}s")
}

impl fmt::Display for UsageRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label())
    }
}
