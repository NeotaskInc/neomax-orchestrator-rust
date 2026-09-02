use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::io::{os_str_to_utf8, path_to_utf8};
use crate::{Error, Result};

use super::guard::RecursionGuard;

pub const NEOMAX_BIN_ENV: &str = "NEOMAX_BIN";
pub const NEOMAX_TOOL_MANIFEST_ENV: &str = "NEOMAX_TOOL_MANIFEST";
pub const NEOMAX_TOOL_DEPTH_ENV: &str = "NEOMAX_TOOL_DEPTH";
pub const NEOMAX_TOOL_MAX_DEPTH_ENV: &str = "NEOMAX_TOOL_MAX_DEPTH";
pub const NEOMAX_TOOL_INSTRUCTION_ENV: &str = "NEOMAX_TOOL_INSTRUCTION";
pub const NEOMAX_TOOL_POLICY_ENV: &str = "NEOMAX_TOOL_POLICY";
/// Explicit opt-in required before a process may receive the full tool policy.
pub const NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV: &str = "NEOMAX_ALLOW_FULL_TOOL_POLICY";

#[derive(Debug, Clone)]
pub struct EnvironmentInput<'a> {
    pub executable: &'a Path,
    pub manifest_path: &'a Path,
    pub install_bin: Option<&'a Path>,
    pub existing_path: Option<&'a OsStr>,
    pub guard: RecursionGuard,
    pub role: super::LaunchRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEnvironment {
    variables: BTreeMap<String, String>,
    instruction: &'static str,
}

impl ToolEnvironment {
    pub fn variables(&self) -> &BTreeMap<String, String> {
        &self.variables
    }

    pub fn instruction(&self) -> &'static str {
        self.instruction
    }

    pub fn into_variables(self) -> BTreeMap<String, String> {
        self.variables
    }

    pub fn extend_into(&self, target: &mut BTreeMap<String, String>) {
        target.extend(self.variables.clone());
    }
}

pub fn build_environment(input: EnvironmentInput<'_>) -> Result<ToolEnvironment> {
    validate_absolute_path("Neomax executable", input.executable)?;
    validate_absolute_path("Neomax tool manifest", input.manifest_path)?;
    let executable = path_to_utf8("Neomax executable path", input.executable)?;
    let manifest = path_to_utf8("Neomax tool manifest path", input.manifest_path)?;
    let child_guard = if input.role.is_orchestrator() {
        input.guard
    } else {
        input.guard.enter()?
    };
    let path = augment_path(input.existing_path, input.executable, input.install_bin)?;
    let path = os_str_to_utf8("augmented PATH", path.as_os_str())?;

    let instruction = super::manifest::tool_instruction_for(input.role);
    let variables = BTreeMap::from([
        (NEOMAX_BIN_ENV.into(), executable.into()),
        (NEOMAX_TOOL_MANIFEST_ENV.into(), manifest.into()),
        (
            NEOMAX_TOOL_DEPTH_ENV.into(),
            child_guard.depth().to_string(),
        ),
        (
            NEOMAX_TOOL_MAX_DEPTH_ENV.into(),
            child_guard.max_depth().to_string(),
        ),
        (NEOMAX_TOOL_INSTRUCTION_ENV.into(), instruction.into()),
        (
            NEOMAX_TOOL_POLICY_ENV.into(),
            input.role.policy_name().into(),
        ),
        ("PATH".into(), path.into()),
    ]);

    Ok(ToolEnvironment {
        variables,
        instruction,
    })
}

pub fn augment_path(
    existing_path: Option<&OsStr>,
    executable: &Path,
    install_bin: Option<&Path>,
) -> Result<OsString> {
    let mut prefixes = Vec::new();
    if let Some(parent) = executable.parent() {
        prefixes.push(parent.to_path_buf());
    }
    if let Some(install_bin) = install_bin {
        if let Some(parent) = install_bin.parent() {
            prefixes.push(parent.to_path_buf());
        }
    }

    let mut entries = Vec::<PathBuf>::new();
    for prefix in prefixes {
        validate_prefix(&prefix)?;
        if !entries.contains(&prefix) {
            entries.push(prefix);
        }
    }
    if let Some(existing_path) = existing_path {
        for entry in env::split_paths(existing_path) {
            if safe_path_entry(&entry) && !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    env::join_paths(entries).map_err(|error| {
        Error::InvalidArgument(format!("cannot construct the Neomax worker PATH: {error}"))
    })
}

fn safe_path_entry(path: &Path) -> bool {
    path.is_absolute() && !crate::io::is_rooted_but_not_absolute(path)
}

fn validate_prefix(prefix: &Path) -> Result<()> {
    if !prefix.is_absolute() {
        return Err(Error::InvalidArgument(format!(
            "Neomax executable directory must be absolute: {}",
            prefix.display()
        )));
    }
    if prefix.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(
            "Neomax executable directory cannot be empty".into(),
        ));
    }
    Ok(())
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(Error::InvalidArgument(format!(
            "{label} path must be absolute: {}",
            path.display()
        )));
    }
    Ok(())
}
