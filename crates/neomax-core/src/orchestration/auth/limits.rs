use std::time::Duration;

use crate::io::ReadLimits;

pub(crate) const MAX_CREDENTIAL_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_BACKUP_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_ROTATION_LOG_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const CREDENTIAL_READ_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const BACKUP_READ_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const ROTATION_LOG_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn credential_read_limits() -> ReadLimits {
    ReadLimits::new(MAX_CREDENTIAL_BYTES, CREDENTIAL_READ_TIMEOUT)
        .expect("credential read limits are valid")
}

pub(crate) fn backup_read_limits() -> ReadLimits {
    ReadLimits::new(MAX_BACKUP_BYTES, BACKUP_READ_TIMEOUT).expect("backup read limits are valid")
}

pub(crate) fn rotation_log_read_limits() -> ReadLimits {
    ReadLimits::new(MAX_ROTATION_LOG_BYTES, ROTATION_LOG_READ_TIMEOUT)
        .expect("rotation log read limits are valid")
}
