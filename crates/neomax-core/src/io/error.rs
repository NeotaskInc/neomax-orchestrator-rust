use std::io;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum BoundedIoError {
    #[error("invalid bounded I/O limit: {0}")]
    InvalidLimit(String),
    #[error("not found: {path}")]
    NotFound { path: PathBuf },
    #[error("corrupt file {path}: {message}")]
    Corrupt { path: PathBuf, message: String },
    #[error("{operation} timed out after {timeout_ms}ms")]
    Timeout { operation: String, timeout_ms: u128 },
    #[error("{operation} exceeded its {limit}-byte limit")]
    Truncated { operation: String, limit: usize },
    #[error("process {program} failed with exit code {code:?}")]
    ProcessFailed { program: String, code: Option<i32> },
    #[error("could not spawn {program}: {source}")]
    Spawn { program: String, source: io::Error },
    #[error("{operation} failed: {source}")]
    Io {
        operation: String,
        source: io::Error,
    },
    #[error("{operation} failed because its output reader was unavailable")]
    MissingPipe { operation: String },
}

pub type Result<T> = std::result::Result<T, BoundedIoError>;

impl BoundedIoError {
    pub(crate) fn io(operation: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            operation: operation.into(),
            source,
        }
    }

    pub(crate) fn timeout(operation: impl Into<String>, timeout: Duration) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_ms: timeout.as_millis(),
        }
    }
}

impl From<BoundedIoError> for crate::Error {
    fn from(error: BoundedIoError) -> Self {
        match error {
            BoundedIoError::InvalidLimit(message) => Self::InvalidArgument(message),
            BoundedIoError::NotFound { path } => Self::NotFound(path.display().to_string()),
            BoundedIoError::Corrupt { path, message } => Self::InvalidState { path, message },
            BoundedIoError::Timeout {
                operation,
                timeout_ms,
            } => Self::Message(format!("{operation} timed out after {timeout_ms}ms")),
            BoundedIoError::Truncated { operation, limit } => {
                Self::Message(format!("{operation} exceeded its {limit}-byte limit"))
            }
            BoundedIoError::ProcessFailed { program, code } => {
                Self::Message(format!("process {program} failed with exit code {code:?}"))
            }
            BoundedIoError::Spawn { source, .. } | BoundedIoError::Io { source, .. } => {
                Self::Io(source)
            }
            BoundedIoError::MissingPipe { operation } => Self::Message(format!(
                "{operation} failed because its output reader was unavailable"
            )),
        }
    }
}
