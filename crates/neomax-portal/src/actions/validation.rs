use anyhow::{Result, bail};
use neomax_core::config::Engine;

const MAX_ACCOUNT_LENGTH: usize = 16;
const MAX_RUN_ID_LENGTH: usize = 160;

pub(crate) fn parse_engine(value: &str) -> Result<Engine> {
    value.parse().map_err(|error| anyhow::anyhow!("{error}"))
}

pub(crate) fn validate_account(value: &str) -> Result<String> {
    if value.len() > MAX_ACCOUNT_LENGTH
        || value.is_empty()
        || (value != "orch"
            && (value.starts_with('0')
                || !value.chars().all(|character| character.is_ascii_digit())))
    {
        bail!("invalid account identifier")
    }
    if value != "orch" && value.parse::<u32>().unwrap_or(0) == 0 {
        bail!("invalid account identifier")
    }
    Ok(value.to_owned())
}

pub(crate) fn validate_run_id(value: &str) -> Result<String> {
    if value.is_empty()
        || value.len() > MAX_RUN_ID_LENGTH
        || matches!(value, "." | "..")
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        bail!("invalid run id")
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_identifiers_are_bounded_and_numeric() {
        assert!(validate_account("orch").is_ok());
        assert!(validate_account("12").is_ok());
        assert!(validate_account("0").is_err());
        assert!(validate_account("1/2").is_err());
    }

    #[test]
    fn run_identifiers_reject_path_traversal() {
        assert!(validate_run_id("20260823-120000-123").is_ok());
        assert!(validate_run_id("../../secret").is_err());
        assert!(validate_run_id("abc/def").is_err());
    }
}
