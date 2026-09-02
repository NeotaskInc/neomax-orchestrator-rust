use anyhow::{Result, bail};
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::sessions::SessionRecord;

use super::super::discovery::SessionInventory;
use super::model::ResumeTarget;

pub(super) fn ensure_owner_available(
    runtime: &ProviderRuntime,
    inventory: &SessionInventory,
    target: &ResumeTarget,
) -> Result<ResumeTarget> {
    let record = inventory
        .all_records
        .iter()
        .find(|record| {
            !record.is_child()
                && record.engine == target.engine
                && record.account == target.account
                && record.id == target.session_id
        })
        .ok_or_else(|| anyhow::anyhow!("resumable session owner metadata is missing"))?;
    let owner = inventory
        .owner(record)
        .ok_or_else(|| anyhow::anyhow!("resumable session owner profile is missing"))?;
    let provider = runtime
        .catalog()
        .providers
        .get(&target.engine)
        .ok_or_else(|| anyhow::anyhow!("provider {} is unavailable", target.engine))?;
    let profile = provider
        .profiles
        .iter()
        .find(|profile| profile.account == target.account && profile.path == owner)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "session {} belongs to {} account {}, which is not available",
                target.session_id,
                target.engine,
                target.account
            )
        })?;
    if !profile.eligibility.orchestrator_eligible {
        bail!(
            "session {} belongs to {} account {}, which is not eligible for an orchestrator resume",
            target.session_id,
            target.engine,
            target.account
        );
    }
    Ok(target.clone())
}

pub(super) fn owner_available(
    inventory: &SessionInventory,
    runtime: &ProviderRuntime,
    record: &SessionRecord,
) -> bool {
    let Some(owner) = inventory.owner(record) else {
        return false;
    };
    runtime
        .catalog()
        .providers
        .get(&record.engine)
        .and_then(|provider| {
            provider
                .profiles
                .iter()
                .find(|profile| profile.account == record.account && profile.path == owner)
        })
        .is_some_and(|profile| profile.eligibility.orchestrator_eligible)
}
