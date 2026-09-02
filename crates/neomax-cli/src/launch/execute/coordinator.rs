use neomax_core::accounts::{AccountControlStore, AccountInventory, RotationClaimStore};
use neomax_core::git::pull_request::GitHubPullRequestAdapter;
use neomax_core::orchestration::continuation::FilesystemContinuation;
use neomax_core::providers::ProviderRegistry;
use neomax_core::runs::coordinator::{NativeAttemptRunner, RunCoordinator};
use neomax_core::runs::lifecycle::{
    ManagedRunWorktreeFinalizer, PullRequestFinalizer, RunFinalizer,
};
use neomax_core::runs::{
    EventStore, HistoryStore, RunLiveWorkSource, RunRecord, RunStore, SystemProcessProbe,
};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{EffectiveSettings, StatePaths, WorkerScope};

pub(super) fn execute(
    providers: &ProviderRegistry,
    settings: &EffectiveSettings,
    paths: &StatePaths,
    scope: &WorkerScope,
    runs: &RunStore,
    run: &mut RunRecord,
) -> neomax_core::Result<neomax_core::runs::lifecycle::Finalization> {
    let pull_requests = GitHubPullRequestAdapter::default();
    execute_with_pull_request(
        providers,
        settings,
        paths,
        scope,
        runs,
        run,
        Some(&pull_requests),
    )
}

pub(crate) fn execute_with_pull_request(
    providers: &ProviderRegistry,
    settings: &EffectiveSettings,
    paths: &StatePaths,
    scope: &WorkerScope,
    runs: &RunStore,
    run: &mut RunRecord,
    pull_requests: Option<&dyn PullRequestFinalizer>,
) -> neomax_core::Result<neomax_core::runs::lifecycle::Finalization> {
    let usage = UsageCacheStore::new(&paths.usage);
    let controls = AccountControlStore::new(&paths.cooldowns, &paths.paused);
    let claims = RotationClaimStore::new(&paths.rotation_claims, &paths.rotation_lock);
    let continuation = FilesystemContinuation::in_paths(paths, Some(paths.usage.clone()));
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(runs, &probe);
    let inventory = AccountInventory {
        providers,
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    let events = EventStore::with_legacy_directory(&paths.run_events, &paths.events);
    let history = HistoryStore::new(
        &paths.history_db,
        &paths.logs,
        &paths.history_logs,
        &paths.history_pending,
    );
    let worktrees = ManagedRunWorktreeFinalizer::new(&paths.worktrees);
    let finalizer = RunFinalizer {
        runs,
        events: &events,
        history: &history,
        controls: &controls,
        worktrees: &worktrees,
        pull_requests,
    };
    let attempts = NativeAttemptRunner {
        providers,
        settings,
        paths,
        runs,
        quota: &usage,
    };
    let coordinator = RunCoordinator {
        attempts: &attempts,
        inventory: &inventory,
        runs,
        events: &events,
        controls: &controls,
        finalizer: &finalizer,
        scope,
        selection: neomax_core::accounts::SelectionPolicy::from_settings(settings),
        clock: &neomax_core::runs::coordinator::SystemClock,
        default_cooldown: std::time::Duration::from_secs(30 * 60),
        continuation: Some(&continuation),
        claims: Some(&claims),
    };
    let model_overrides =
        neomax_core::settings::process_environment_model_overrides(&settings.config_path)?;
    coordinator.execute_with_model_resolver(run, &model_overrides)
}
