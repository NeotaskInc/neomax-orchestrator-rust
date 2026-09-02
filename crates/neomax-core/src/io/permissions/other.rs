use std::fs::File;
use std::path::Path;

use crate::{Error, Result};

fn unsupported<T>() -> Result<T> {
    Err(Error::InvalidArgument(
        "private filesystem permissions are unsupported on this platform".into(),
    ))
}

pub(super) fn set_private_path(_path: &Path) -> Result<()> {
    unsupported()
}

pub(super) fn set_private_open_path(_file: &File, _path: &Path) -> Result<()> {
    unsupported()
}

pub(super) fn set_private_directory(_path: &Path) -> Result<()> {
    unsupported()
}

pub(super) fn verify_private_path(_path: &Path) -> Result<()> {
    unsupported()
}
