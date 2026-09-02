use crate::{Error, Result};

use super::environment::{NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV, NEOMAX_TOOL_POLICY_ENV};
use super::manifest::ToolManifest;
use super::role::LaunchRole;
use super::types::{CommandClass, ManifestCommand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPolicy {
    read_only: bool,
    mutating: bool,
    destructive: bool,
    external: bool,
}

impl ToolPolicy {
    pub const fn read_only() -> Self {
        Self {
            read_only: true,
            mutating: false,
            destructive: false,
            external: false,
        }
    }

    pub const fn worker() -> Self {
        Self {
            read_only: true,
            mutating: true,
            destructive: false,
            external: false,
        }
    }

    pub const fn orchestrator() -> Self {
        Self {
            read_only: true,
            mutating: true,
            destructive: false,
            external: true,
        }
    }

    pub const fn full() -> Self {
        Self {
            read_only: true,
            mutating: true,
            destructive: true,
            external: true,
        }
    }

    /// Resolve a policy name from the current process contract.
    ///
    /// The destructive-capable `full` policy is intentionally unavailable to
    /// ordinary callers. It is accepted only when the caller explicitly sets
    /// `NEOMAX_ALLOW_FULL_TOOL_POLICY` to a recognized opt-in value.
    pub fn from_name(name: &str) -> Result<Self> {
        let allow_full = std::env::var(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV)
            .ok()
            .as_deref()
            .is_some_and(Self::full_policy_opted_in);
        Self::from_name_with_full(name, allow_full)
    }

    /// Resolve a policy with an explicit full-policy decision supplied by the
    /// launch boundary. This keeps environment parsing out of child policy
    /// construction and makes the dangerous escalation auditable in tests.
    pub fn from_name_with_full(name: &str, allow_full: bool) -> Result<Self> {
        match name {
            "worker" => Ok(Self::worker()),
            "orchestrator" => Ok(Self::orchestrator()),
            "full" if allow_full => Ok(Self::full()),
            "full" => Err(Error::Conflict(format!(
                "full Neomax tool policy requires {NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV}=1"
            ))),
            other => Err(Error::InvalidArgument(format!(
                "unknown Neomax tool policy: {other}"
            ))),
        }
    }

    pub fn for_role(role: LaunchRole) -> Self {
        match role {
            LaunchRole::Orchestrator => Self::orchestrator(),
            LaunchRole::Worker => Self::worker(),
        }
    }

    pub const fn as_name(self) -> &'static str {
        match (
            self.read_only,
            self.mutating,
            self.destructive,
            self.external,
        ) {
            (true, true, true, true) => "full",
            (true, true, false, true) => "orchestrator",
            (true, true, false, false) => "worker",
            _ => "unknown",
        }
    }

    pub const fn is_full(self) -> bool {
        self.destructive && self.external
    }

    pub fn allows_role(self, role: LaunchRole) -> bool {
        self.is_full() || self == Self::for_role(role)
    }

    pub fn full_policy_opted_in(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }

    /// Resolve the policy carried by a prepared agent environment.
    ///
    /// An absent policy is not an error here: callers receive the least
    /// privilege policy for their launch role. A present policy is always
    /// parsed and validated, including the explicit full-policy opt-in.
    pub fn from_environment(
        environment: &std::collections::BTreeMap<String, String>,
        role: LaunchRole,
    ) -> Result<Self> {
        let Some(name) = environment.get(NEOMAX_TOOL_POLICY_ENV) else {
            return Ok(Self::for_role(role));
        };
        let allow_full = environment
            .get(NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV)
            .is_some_and(|value| Self::full_policy_opted_in(value));
        let policy = Self::from_name_with_full(name, allow_full)?;
        if !policy.allows_role(role) {
            return Err(Error::Conflict(format!(
                "Neomax tool policy {} is not valid for {} launches",
                policy.as_name(),
                role.as_str()
            )));
        }
        Ok(policy)
    }

    pub const fn allows(self, class: CommandClass) -> bool {
        match class {
            CommandClass::ReadOnly => self.read_only,
            CommandClass::Mutating => self.mutating,
            CommandClass::Destructive => self.destructive,
            CommandClass::External => self.external,
        }
    }

    pub fn authorize<'a>(
        self,
        manifest: &'a ToolManifest,
        command: &str,
    ) -> Result<AuthorizedCommand<'a>> {
        let command = manifest.command(command).ok_or_else(|| {
            Error::NotFound(format!("tool command is not in the manifest: {command}"))
        })?;
        if !self.allows(command.class) {
            return Err(Error::Conflict(format!(
                "tool policy denies {} command {}",
                command.class.as_str(),
                command.command
            )));
        }
        Ok(AuthorizedCommand { command })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizedCommand<'a> {
    command: &'a ManifestCommand,
}

impl<'a> AuthorizedCommand<'a> {
    pub const fn command(self) -> &'a ManifestCommand {
        self.command
    }
}
