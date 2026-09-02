use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::providers::catalog::AuthMethod;
use neomax_core::providers::catalog::ProfileEligibility;

use super::profile::{AccountContext, AccountProfile, inspect_profile};
use crate::model::AccountView;

pub(crate) fn account_view(
    profile: &ProviderProfile,
    engine: Engine,
    eligibility: Option<ProfileEligibility>,
    auth_methods: Option<&[AuthMethod]>,
    binary_available: bool,
    context: &AccountContext<'_>,
) -> anyhow::Result<AccountView> {
    Ok(render_profile(inspect_profile(
        profile,
        engine,
        eligibility,
        auth_methods,
        binary_available,
        context,
    )?))
}

pub(crate) fn render_profile(profile: AccountProfile) -> AccountView {
    let AccountProfile {
        account,
        path,
        reserved,
        rotate_advised,
        authenticated,
        worker_eligible,
        eligibility,
        email,
        plan,
        display_name,
        auth_method,
        live,
        workers,
        mains,
        subagents,
        cooldown_until,
        paused,
        token_expired,
        usage,
        telemetry,
        capabilities,
    } = profile;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&account)
        .to_string();
    AccountView {
        n: account.clone(),
        dir: path,
        name,
        role: if reserved || account == "orch" {
            "orchestrator".into()
        } else {
            "worker".into()
        },
        reserved,
        rotate_advised,
        authenticated,
        worker_eligible,
        eligibility,
        email,
        plan,
        display_name,
        auth_method,
        live,
        workers,
        mains,
        subagents,
        agents: workers.saturating_add(mains).saturating_add(subagents),
        cooldown_until,
        paused,
        token_expired,
        usage,
        telemetry,
        capabilities,
        duplicate_of: Vec::new(),
    }
}
