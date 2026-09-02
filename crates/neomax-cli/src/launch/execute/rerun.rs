use neomax_core::providers::ProviderRegistry;
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::{RunRecord, RunStatus, RunStore};
use neomax_core::{EffectiveSettings, StatePaths, WorkerScope};

pub(crate) fn execute_record_with_runtime(
    runtime: &ProviderRuntime,
    paths: &StatePaths,
    settings: &EffectiveSettings,
    run: &mut RunRecord,
) -> neomax_core::Result<RunStatus> {
    execute_record_with_registry(runtime.registry(), paths, settings, run)
}

fn execute_record_with_registry(
    providers: &ProviderRegistry,
    paths: &StatePaths,
    settings: &EffectiveSettings,
    run: &mut RunRecord,
) -> neomax_core::Result<RunStatus> {
    let scope = run
        .extra
        .get("worker_scope")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<WorkerScope>().ok())
        .unwrap_or_else(WorkerScope::all);
    let runs = RunStore::new(&paths.runs);
    let finalization = super::coordinator::execute(providers, settings, paths, &scope, &runs, run)?;
    Ok(finalization.status)
}
