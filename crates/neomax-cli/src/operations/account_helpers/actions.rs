mod identity;
mod login;
mod models;
mod session;
mod shared;
mod status;

use anyhow::Result;

use crate::context::RuntimeContext;

use super::process::ProcessPort;
use super::profiles::AuthPort;
use super::request::{AccountHelperRequest, AccountOperation};

pub(super) fn dispatch(
    request: &AccountHelperRequest,
    context: &RuntimeContext,
    auth: &dyn AuthPort,
    process: &dyn ProcessPort,
) -> Result<()> {
    match &request.operation {
        AccountOperation::Status => status::execute(request, context, auth),
        AccountOperation::Login { .. } => login::execute(request, context, auth, process),
        AccountOperation::Logout => session::logout(request, context, auth, process),
        AccountOperation::Run => session::run(request, context, auth, process),
        AccountOperation::Models => models::execute(request, context, auth, process),
        AccountOperation::Whoami => identity::execute(request, context, auth, process),
    }
}

#[cfg(test)]
use super::process::ProcessOutcome;
#[cfg(test)]
use super::profiles::ManagedProfile;
#[cfg(test)]
use neomax_core::Engine;

#[cfg(test)]
pub(super) fn codex_whoami_output(profile: &ManagedProfile, outcome: &ProcessOutcome) -> String {
    identity::codex_whoami_output(profile, outcome)
}

#[cfg(test)]
pub(super) fn grok_whoami_output(
    profile: &ManagedProfile,
    identity: Option<&neomax_core::providers::catalog::GrokAuthIdentity>,
) -> String {
    identity::grok_whoami_output(profile, identity)
}

#[cfg(test)]
pub(super) fn duplicate_codex_warnings(engine: Engine, profiles: &[ManagedProfile]) -> Vec<String> {
    status::duplicate_codex_warnings(engine, profiles)
}

#[cfg(test)]
pub(super) fn status_profiles(
    engine: Engine,
    profiles: Vec<ManagedProfile>,
    home: &std::path::Path,
    cwd: &std::path::Path,
) -> anyhow::Result<Vec<ManagedProfile>> {
    status::status_profiles(engine, profiles, home, cwd)
}
