use std::collections::BTreeSet;

use crate::config::Engine;
use crate::{Error, Result};

use super::OwnerLiveness;
use super::limits::AdmissionLimits;
use super::rejection::AdmissionRejection;
use super::request::AdmissionRequest;
use super::schema::AdmissionState;

pub(super) fn check_capacity(
    state: &AdmissionState,
    request: &AdmissionRequest,
    limits: &AdmissionLimits,
    replacing: Option<&str>,
) -> Result<()> {
    let leases = state
        .leases
        .iter()
        .filter(|lease| replacing != Some(lease.id.as_str()))
        .collect::<Vec<_>>();
    let fleet_active = u32::try_from(leases.len()).unwrap_or(u32::MAX);
    if let Some(maximum) = limits.fleet_cap {
        if fleet_active >= maximum {
            return Err(Error::Message(
                AdmissionRejection::Fleet {
                    active: fleet_active,
                    maximum,
                }
                .to_string(),
            ));
        }
    }

    if limits.task_cap != 0 {
        let mut tasks = BTreeSet::new();
        for lease in &leases {
            tasks.insert(lease.task.as_str());
        }
        if !tasks.contains(request.task_id.as_str())
            && u32::try_from(tasks.len()).unwrap_or(u32::MAX) >= limits.task_cap
        {
            return Err(Error::Message(
                AdmissionRejection::Task {
                    active: u32::try_from(tasks.len()).unwrap_or(u32::MAX),
                    maximum: limits.task_cap,
                }
                .to_string(),
            ));
        }
    }

    if let Some(engine) = request.engine {
        let provider_active = leases
            .iter()
            .filter(|lease| lease.engine == Some(engine))
            .count();
        if let Some(maximum) = limits.provider_cap {
            if u32::try_from(provider_active).unwrap_or(u32::MAX) >= maximum {
                return Err(Error::Message(
                    AdmissionRejection::Provider {
                        engine,
                        active: u32::try_from(provider_active).unwrap_or(u32::MAX),
                        maximum,
                    }
                    .to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn check_bound_capacity(
    state: &AdmissionState,
    lease_id: &str,
    engine: Engine,
    account: &str,
    session: &str,
    limits: &AdmissionLimits,
) -> Result<()> {
    let leases = state
        .leases
        .iter()
        .filter(|lease| lease.id != lease_id)
        .collect::<Vec<_>>();
    let provider_active = leases
        .iter()
        .filter(|lease| lease.engine == Some(engine))
        .count();
    if let Some(maximum) = limits.provider_cap {
        if u32::try_from(provider_active).unwrap_or(u32::MAX) >= maximum {
            return Err(Error::Message(
                AdmissionRejection::Provider {
                    engine,
                    active: u32::try_from(provider_active).unwrap_or(u32::MAX),
                    maximum,
                }
                .to_string(),
            ));
        }
    }
    let account_active = leases
        .iter()
        .filter(|lease| lease.engine == Some(engine) && lease.account.as_deref() == Some(account))
        .count();
    if u32::try_from(account_active).unwrap_or(u32::MAX) >= limits.lanes_per_account {
        return Err(Error::Message(
            AdmissionRejection::AccountLanes {
                account: account.into(),
                active: u32::try_from(account_active).unwrap_or(u32::MAX),
                maximum: limits.lanes_per_account,
            }
            .to_string(),
        ));
    }
    let sessions = leases
        .iter()
        .filter(|lease| lease.engine == Some(engine) && lease.account.as_deref() == Some(account))
        .filter_map(|lease| lease.session.as_deref())
        .collect::<BTreeSet<_>>();
    let session_active = u32::try_from(sessions.len()).unwrap_or(u32::MAX);
    let is_new_session = !sessions.contains(session);
    if is_new_session && session_active >= limits.sessions_per_account {
        return Err(Error::Message(
            AdmissionRejection::AccountSessions {
                account: account.into(),
                active: session_active,
                maximum: limits.sessions_per_account,
            }
            .to_string(),
        ));
    }
    Ok(())
}

pub(super) fn reap(
    state: &mut AdmissionState,
    now: f64,
    limits: &AdmissionLimits,
    liveness: &dyn OwnerLiveness,
) {
    state.leases.retain(|lease| {
        now.is_finite()
            && lease.created_at.is_finite()
            && now - lease.created_at <= limits.lease_ttl_seconds
            && liveness.is_live(lease.owner_pid)
    });
    state.version = super::constants::STATE_VERSION;
}
