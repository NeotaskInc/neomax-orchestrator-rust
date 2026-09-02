use std::collections::BTreeMap;

use crate::{Error, Result};

use super::schema::ConcurrencySettings;

pub(super) fn first_environment_value<'a, const N: usize>(
    environment: &'a BTreeMap<String, String>,
    keys: [&'static str; N],
) -> Option<(&'static str, &'a str)> {
    keys.into_iter()
        .find_map(|key| environment.get(key).map(|value| (key, value.as_str())))
}

pub(super) fn parse_positive(name: &str, value: &str) -> Result<u32> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| Error::InvalidArgument(format!("{name} must be a positive integer")))?;
    if parsed == 0 {
        return Err(Error::InvalidArgument(format!(
            "{name} must be a positive integer"
        )));
    }
    Ok(parsed)
}

pub(super) fn parse_non_negative(name: &str, value: &str) -> Result<u32> {
    value
        .parse::<u32>()
        .map_err(|_| Error::InvalidArgument(format!("{name} must be a non-negative integer")))
}

pub(super) fn parse_positive_seconds(name: &str, value: &str) -> Result<f64> {
    let parsed = value.parse::<f64>().map_err(|_| {
        Error::InvalidArgument(format!("{name} must be a positive number of seconds"))
    })?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(Error::InvalidArgument(format!(
            "{name} must be a positive number of seconds"
        )));
    }
    Ok(parsed)
}

pub(super) fn validate_concurrency(settings: &ConcurrencySettings) -> Result<()> {
    if settings.max_subagents == 0
        || settings.max_sessions_per_account == 0
        || settings.lanes_per_account == 0
    {
        return Err(Error::InvalidArgument(
            "concurrency limits other than max_tasks must be positive".into(),
        ));
    }
    if !settings.queue_ttl_seconds.is_finite() || settings.queue_ttl_seconds <= 0.0 {
        return Err(Error::InvalidArgument(
            "queue_ttl_seconds must be finite and positive".into(),
        ));
    }
    Ok(())
}
