use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::accounts::{select_account, AccountSelector, AccountSnapshot, SelectionPolicy};
use crate::runs::{RunRecord, RunStatus};
use crate::WorkerScope;

use super::order::cross_provider_order;
use super::types::{FailoverDecision, FailoverStop, FailoverTarget};

pub fn plan_failover(
    run: &RunRecord,
    status: RunStatus,
    accounts: &[AccountSnapshot],
    scope: &WorkerScope,
    now: DateTime<Utc>,
    policy: &SelectionPolicy,
) -> FailoverDecision {
    if !matches!(status, RunStatus::Limit | RunStatus::Error) {
        return FailoverDecision::Stop(FailoverStop::TerminalStatus);
    }
    if run.no_failover {
        return FailoverDecision::Stop(FailoverStop::Disabled);
    }
    if run.resumed {
        return FailoverDecision::Stop(FailoverStop::ResumedRun);
    }
    let pool_size = accounts
        .iter()
        .filter(|account| scope.contains(account.engine) && !account.reserved)
        .count();
    if usize::try_from(run.attempt).unwrap_or(usize::MAX) >= pool_size {
        return FailoverDecision::Stop(FailoverStop::AttemptsExhausted);
    }

    let excluded = excluded_profiles(run);
    if let Some(account) = select_for_engine(run.engine, accounts, &excluded, now, policy) {
        return FailoverDecision::Continue(FailoverTarget {
            account,
            crosses_provider: false,
        });
    }
    if status == RunStatus::Limit {
        for engine in cross_provider_order(run.engine, scope) {
            if let Some(account) = select_for_engine(engine, accounts, &excluded, now, policy) {
                return FailoverDecision::Continue(FailoverTarget {
                    account,
                    crosses_provider: true,
                });
            }
        }
    }
    FailoverDecision::Stop(FailoverStop::NoEligibleAccount)
}

fn select_for_engine(
    engine: crate::Engine,
    accounts: &[AccountSnapshot],
    excluded: &BTreeSet<PathBuf>,
    now: DateTime<Utc>,
    policy: &SelectionPolicy,
) -> Option<AccountSnapshot> {
    let candidates = accounts
        .iter()
        .filter(|account| account.engine == engine)
        .cloned()
        .collect::<Vec<_>>();
    select_account(
        &candidates,
        &AccountSelector::Auto,
        excluded,
        &BTreeMap::new(),
        now,
        policy,
    )
    .ok()
    .map(|decision| decision.account.clone())
}

fn excluded_profiles(run: &RunRecord) -> BTreeSet<PathBuf> {
    run.tried
        .iter()
        .cloned()
        .chain(std::iter::once(run.profile.clone()))
        .collect()
}
