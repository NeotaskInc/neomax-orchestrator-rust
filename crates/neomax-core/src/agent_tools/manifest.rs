use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

use super::commands::CANONICAL_COMMANDS;
use super::types::{CommandClass, CommandFamily, ManifestCommand};

pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const TOOL_INSTRUCTION: &str = "Use NEOMAX_BIN with commands listed in NEOMAX_TOOL_MANIFEST. Follow the caller policy and do not start another worker.";
pub const ORCHESTRATOR_TOOL_INSTRUCTION: &str = "Use NEOMAX_BIN with commands listed in NEOMAX_TOOL_MANIFEST. Dispatch and inspect work through the canonical tool surface, preserve project instructions, and honor NEOMAX_TOOL_POLICY and recursion limits.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolManifest {
    pub schema_version: u16,
    pub protocol: String,
    pub commands: Vec<ManifestCommand>,
}

pub type ToolManifest = AgentToolManifest;

impl AgentToolManifest {
    pub fn canonical() -> Self {
        let mut commands = CANONICAL_COMMANDS
            .iter()
            .copied()
            .map(ManifestCommand::from_canonical)
            .collect::<Vec<_>>();
        commands.sort_by(|left, right| {
            (left.family, left.command.as_str()).cmp(&(right.family, right.command.as_str()))
        });
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            protocol: "neomax-tool-v1".into(),
            commands,
        }
    }

    pub fn command(&self, name: &str) -> Option<&ManifestCommand> {
        self.commands.iter().find(|command| command.command == name)
    }

    pub fn validate(&self) -> Result<()> {
        crate::registry::validate_architecture()?;
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(Error::InvalidArgument(format!(
                "unsupported tool manifest schema {}",
                self.schema_version
            )));
        }
        if self.protocol != "neomax-tool-v1" {
            return Err(Error::InvalidArgument(format!(
                "unsupported tool manifest protocol {}",
                self.protocol
            )));
        }
        if self.commands.is_empty() {
            return Err(Error::InvalidArgument(
                "tool manifest has no commands".into(),
            ));
        }

        let canonical = canonical_commands();
        let mut seen = BTreeSet::new();
        let mut previous = None;
        for command in &self.commands {
            if command.command.trim() != command.command || command.command.is_empty() {
                return Err(Error::InvalidArgument(format!(
                    "invalid tool command name {:?}",
                    command.command
                )));
            }
            if command.command.chars().any(char::is_control) {
                return Err(Error::InvalidArgument(format!(
                    "tool command contains a control character: {:?}",
                    command.command
                )));
            }
            let key = (command.family, command.command.clone());
            let Some((canonical_class, canonical_summary)) = canonical.get(&key) else {
                return Err(Error::InvalidArgument(format!(
                    "tool command is not canonical: {}",
                    command.command
                )));
            };
            if command.class != *canonical_class
                || command.summary.as_str() != canonical_summary.as_str()
            {
                return Err(Error::InvalidArgument(format!(
                    "tool command metadata does not match its canonical definition: {}",
                    command.command
                )));
            }
            if !seen.insert(key.clone()) {
                return Err(Error::InvalidArgument(format!(
                    "duplicate tool command: {}",
                    command.command
                )));
            }
            let order_key = (command.family, command.command.as_str());
            if let Some(previous) = previous {
                if previous >= order_key {
                    return Err(Error::InvalidArgument(
                        "tool manifest commands are not deterministically ordered".into(),
                    ));
                }
            }
            previous = Some(order_key);
        }
        if seen.len() != canonical.len() {
            let missing = canonical
                .keys()
                .filter(|key| !seen.contains(*key))
                .map(|(_, command)| command.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::InvalidArgument(format!(
                "tool manifest is missing canonical commands: {missing}"
            )));
        }
        Ok(())
    }

    pub fn is_canonical(&self) -> bool {
        self == &Self::canonical()
    }

    pub fn json_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn tool_instruction(&self) -> &'static str {
        TOOL_INSTRUCTION
    }
}

pub const fn tool_instruction_for(role: super::LaunchRole) -> &'static str {
    match role {
        super::LaunchRole::Orchestrator => ORCHESTRATOR_TOOL_INSTRUCTION,
        super::LaunchRole::Worker => TOOL_INSTRUCTION,
    }
}

fn canonical_commands() -> BTreeMap<(CommandFamily, String), (CommandClass, String)> {
    CANONICAL_COMMANDS
        .iter()
        .map(|command| {
            (
                (command.family, command.command.into()),
                (command.class, command.summary.into()),
            )
        })
        .collect()
}
