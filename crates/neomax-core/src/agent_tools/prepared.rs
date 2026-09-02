use std::collections::BTreeMap;

#[cfg(test)]
use super::NEOMAX_TOOL_POLICY_ENV;
use super::{LaunchRole, ToolPolicy};

/// The immutable tool environment proof attached to a worker launch.
///
/// Construction stays crate-private so provider commands can only receive
/// tool variables produced by the execution preparation boundary.
#[derive(Debug, Clone)]
pub struct PreparedWorkerTools {
    variables: BTreeMap<String, String>,
    role: LaunchRole,
    policy: ToolPolicy,
}

impl PreparedWorkerTools {
    pub(crate) fn new(
        role: LaunchRole,
        policy: ToolPolicy,
        variables: BTreeMap<String, String>,
    ) -> Self {
        Self {
            variables,
            role,
            policy,
        }
    }

    pub fn variables(&self) -> &BTreeMap<String, String> {
        &self.variables
    }

    pub const fn role(&self) -> LaunchRole {
        self.role
    }

    pub const fn policy(&self) -> ToolPolicy {
        self.policy
    }

    #[cfg(test)]
    pub(crate) fn test_fixture() -> Self {
        Self::test_fixture_for(LaunchRole::Worker)
    }

    #[cfg(test)]
    pub(crate) fn test_fixture_for(role: LaunchRole) -> Self {
        let mut variables = BTreeMap::from([
            ("NEOMAX_BIN".into(), "/fixture/bin/neomax".into()),
            (
                "NEOMAX_TOOL_MANIFEST".into(),
                "/fixture/state/agent-tools/manifest.json".into(),
            ),
            ("NEOMAX_TOOL_DEPTH".into(), "1".into()),
            ("NEOMAX_TOOL_MAX_DEPTH".into(), "4".into()),
            (
                "NEOMAX_TOOL_INSTRUCTION".into(),
                super::manifest::tool_instruction_for(role).into(),
            ),
            (NEOMAX_TOOL_POLICY_ENV.into(), role.policy_name().into()),
            ("PATH".into(), "/fixture/bin:/usr/bin:/bin".into()),
        ]);
        variables.insert(
            "NEOMAX_TOOL_DEPTH".into(),
            if role.is_orchestrator() { "0" } else { "1" }.into(),
        );
        Self::new(role, ToolPolicy::for_role(role), variables)
    }
}
