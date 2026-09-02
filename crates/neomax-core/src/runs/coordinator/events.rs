use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::Result;
use crate::accounts::AccountSnapshot;
use crate::runs::{EventStore, RunEvent, RunRecord, RunStatus};

pub(super) fn append_attempt(
    store: &EventStore,
    run: &RunRecord,
    event: &str,
    status: Option<RunStatus>,
    now: DateTime<Utc>,
) -> Result<()> {
    store.append(
        &RunEvent {
            ts: now.timestamp(),
            run: run.id.clone(),
            event: event.into(),
            engine: run.engine,
            account: Some(run.account()),
            status,
            attempt: Some(run.attempt),
            extra: BTreeMap::new(),
        },
        now,
    )
}

pub(super) fn append_failover_with_strategy(
    store: &EventStore,
    run: &RunRecord,
    status: RunStatus,
    target: &AccountSnapshot,
    crosses_provider: bool,
    strategy: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let extra = BTreeMap::from([
        ("reason".into(), status.as_str().into()),
        ("to_engine".into(), target.engine.as_str().into()),
        ("to".into(), target.account.clone().into()),
        ("strategy".into(), strategy.into()),
    ]);
    store.append(
        &RunEvent {
            ts: now.timestamp(),
            run: run.id.clone(),
            event: if crosses_provider {
                "cross_provider_failover".into()
            } else {
                "failover".into()
            },
            engine: run.engine,
            account: Some(run.account()),
            status: Some(status),
            attempt: Some(run.attempt),
            extra,
        },
        now,
    )
}
