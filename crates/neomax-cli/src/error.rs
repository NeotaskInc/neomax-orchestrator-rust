use std::error::Error as StdError;
use std::fmt;

use anyhow::Error;

/// A typed command-line failure. The process boundary uses this marker to
/// distinguish rejected input from failures while executing an accepted
/// request; it must not infer the distinction from rendered text.
#[derive(Debug)]
pub(crate) struct CliError {
    kind: CliErrorKind,
    source: Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliErrorKind {
    Usage,
}

impl CliError {
    pub(crate) fn usage(source: Error) -> Self {
        Self {
            kind: CliErrorKind::Usage,
            source,
        }
    }

    pub(crate) fn kind(&self) -> CliErrorKind {
        self.kind
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl StdError for CliError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

pub(crate) fn usage<T, E>(result: std::result::Result<T, E>) -> anyhow::Result<T>
where
    E: Into<Error>,
{
    result.map_err(usage_error)
}

pub(crate) fn usage_error(error: impl Into<Error>) -> Error {
    Error::new(CliError::usage(error.into()))
}

pub(crate) fn exit_code(error: &Error) -> Option<i32> {
    error.chain().find_map(|cause| {
        let failure = cause.downcast_ref::<CliError>()?;
        match failure.kind() {
            CliErrorKind::Usage => Some(2),
        }
    })
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::{CliError, exit_code, usage, usage_error};

    #[test]
    fn classification_is_typed_and_does_not_inspect_messages() {
        let error = usage_error(anyhow!("runtime-looking text"));
        assert_eq!(exit_code(&error), Some(2));
        assert_eq!(exit_code(&anyhow!("usage: this is not typed")), None);
    }

    #[test]
    fn usage_preserves_the_original_display() {
        let error = usage::<(), _>(Err(anyhow!("--engine requires a value"))).unwrap_err();
        assert_eq!(error.to_string(), "--engine requires a value");
        assert!(error.downcast_ref::<CliError>().is_some());
    }
}
