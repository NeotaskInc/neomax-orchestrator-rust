mod common;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

use std::io;
use std::path::{Path, PathBuf};

use neomax_core::Engine;

pub(super) fn fake_name(engine: Engine) -> &'static str {
    #[cfg(unix)]
    {
        unix::fake_name(engine)
    }
    #[cfg(windows)]
    {
        windows::fake_name(engine)
    }
}

pub(super) fn write_fake_provider(bin_dir: &Path, engine: Engine) -> io::Result<()> {
    let provider = common::provider_name(engine);
    #[cfg(unix)]
    {
        unix::write_fake_provider(bin_dir, engine, provider)
    }
    #[cfg(windows)]
    {
        windows::write_fake_provider(bin_dir, engine, provider)
    }
}

pub(super) fn write_fake_security(bin_dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        unix::write_fake_security(bin_dir)
    }
    #[cfg(windows)]
    {
        windows::write_fake_security(bin_dir)
    }
}

pub(super) fn write_poison_provider(bin_dir: &Path, engine: Engine) -> io::Result<()> {
    let provider = common::provider_name(engine);
    #[cfg(unix)]
    {
        unix::write_poison_provider(bin_dir, provider)
    }
    #[cfg(windows)]
    {
        windows::write_poison_provider(bin_dir, provider)
    }
}

pub(super) fn alias_path(bin_dir: &Path, alias: &str) -> PathBuf {
    #[cfg(unix)]
    {
        unix::alias_path(bin_dir, alias)
    }
    #[cfg(windows)]
    {
        windows::alias_path(bin_dir, alias)
    }
}

pub(super) fn create_alias(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        unix::create_alias(path)
    }
    #[cfg(windows)]
    {
        windows::create_alias(path)
    }
}
