use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::io::{is_rooted_but_not_absolute, os_str_to_utf8, path_to_string};

pub(super) fn absolute_env_path(name: &str, default: PathBuf) -> Result<String> {
    let path = env::var_os(name).map(PathBuf::from).unwrap_or(default);
    if !is_absolute_path(&path) {
        bail!("{name} must be an absolute path")
    }
    path_to_string(name, &path).map_err(Into::into)
}

pub(super) fn require_absolute(name: &str, path: &Path) -> Result<()> {
    if !is_absolute_path(path) {
        bail!("{name} must be an absolute path")
    }
    Ok(())
}

pub(super) fn is_absolute_path(path: &Path) -> bool {
    path.is_absolute() && !is_rooted_but_not_absolute(path)
}

pub(super) fn validated_provider_value(name: &str, raw: &OsString) -> Result<String> {
    let value = os_str_to_utf8(name, raw)?;
    if value.is_empty() || value.chars().any(char::is_control) {
        bail!("{name} must not be empty or contain control characters")
    }
    if name.ends_with("_PROFILES") {
        for path in env::split_paths(raw) {
            if !is_absolute_path(&path) {
                bail!("{name} must contain only absolute paths")
            }
        }
    } else if (name.ends_with("_ORCH")
        || [
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
            "XDG_DATA_HOME",
            "KIMI_CODE_HOME",
            "GROK_HOME",
        ]
        .contains(&name))
        && !is_absolute_path(Path::new(value))
    {
        bail!("{name} must be an absolute path")
    }
    Ok(value.into())
}

pub(super) fn validated_path(raw: Option<&OsStr>, anchors: &[&Path]) -> Result<String> {
    let mut directories = Vec::new();
    if let Some(raw) = raw {
        for entry in env::split_paths(raw) {
            if entry.as_os_str().is_empty() {
                continue;
            }
            if !is_absolute_path(&entry) {
                bail!("PATH must contain only absolute directories")
            }
            if entry.is_dir() {
                let entry = std::fs::canonicalize(entry)?;
                if !directories.contains(&entry) {
                    directories.push(entry);
                }
            }
        }
    }
    for anchor in anchors {
        if !is_absolute_path(anchor) {
            bail!("PATH anchor must be an absolute path")
        }
        if let Some(parent) = anchor.parent() {
            if parent.is_dir() {
                let parent = std::fs::canonicalize(parent)?;
                if !directories.contains(&parent) {
                    directories.push(parent);
                }
            }
        }
    }
    if directories.is_empty() {
        bail!("PATH must contain an existing directory")
    }
    let joined = env::join_paths(directories)
        .map_err(|_| anyhow::anyhow!("PATH contains an invalid service path"))?;
    os_str_to_utf8("PATH", &joined)
        .map(str::to_owned)
        .map_err(Into::into)
}

pub(super) fn env_u64(name: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    let value = env::var(name).unwrap_or_else(|_| default.to_string());
    let parsed = value
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("{name} must be an integer"))?;
    if parsed < min || parsed > max {
        bail!("{name} must be between {min} and {max}");
    }
    Ok(parsed)
}
