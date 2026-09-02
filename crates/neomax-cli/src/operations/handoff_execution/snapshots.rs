use anyhow::Result;
use neomax_core::WorkerScope;
use neomax_core::accounts::{AccountControlStore, AccountInventory, AccountSnapshot};
use neomax_core::providers::ProviderRuntime;
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;

use super::super::selection::context_time;
use crate::context::RuntimeContext;

pub(crate) fn snapshots(
    context: &RuntimeContext,
    runtime: &ProviderRuntime,
) -> Result<Vec<AccountSnapshot>> {
    snapshots_with_registry(context, runtime.registry())
}

fn snapshots_with_registry(
    context: &RuntimeContext,
    providers: &neomax_core::providers::ProviderRegistry,
) -> Result<Vec<AccountSnapshot>> {
    let runs = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let inventory = AccountInventory {
        providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    inventory
        .routing_snapshots(&WorkerScope::all(), context_time(context))
        .map_err(Into::into)
}
