use chrono::{TimeZone, Utc};
use neomax_core::runs::{EventStore, RunRecord};

use crate::context::RuntimeContext;

pub(super) fn append(context: &RuntimeContext, run: &RunRecord, event: &str) {
    let Some(at) = Utc.timestamp_opt(context.now, 0).single() else {
        return;
    };
    let store = EventStore::with_legacy_directory(&context.paths.run_events, &context.paths.events);
    let _ = store.append(
        &neomax_core::runs::RunEvent {
            ts: context.now,
            run: run.id.clone(),
            event: event.into(),
            engine: run.engine,
            account: Some(run.account()),
            status: Some(run.status),
            attempt: Some(run.attempt),
            extra: Default::default(),
        },
        at,
    );
}
