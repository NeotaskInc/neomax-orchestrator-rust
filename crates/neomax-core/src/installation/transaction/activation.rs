use std::path::Path;

use crate::Result;

use super::platform::{is_cross_device, RenameOps};
use super::staging::copy_entry_to;

pub(super) fn move_with_fallback<R: RenameOps>(
    source: &Path,
    target: &Path,
    renamer: &R,
) -> Result<()> {
    match renamer.rename(source, target) {
        Ok(()) => Ok(()),
        Err(error) if is_cross_device(&error) => copy_entry_to(source, target, renamer),
        Err(error) => Err(crate::Error::Io(error)),
    }
}
