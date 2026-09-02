use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::Result;
use neomax_core::orchestration::registry::OrchestratorRecord;
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::runs::RunRecord;
use neomax_core::sessions::artifacts::{ArtifactSource, FsArtifactSource};
use neomax_core::sessions::filters::DiscoveryContext;
use neomax_core::sessions::{PortalSnapshot, flatten_native_children};
use neomax_core::{Engine, Result as CoreResult};

use crate::context::RuntimeContext;

const SESSION_LOOKBACK_SECONDS: i64 = 3 * 86_400;
const SESSION_ARTIFACT_MAX_BYTES: usize = 128 * 1024 * 1024;

pub(crate) fn discover(
    context: &RuntimeContext,
    runtime: &ProviderRuntime,
    runs: &[RunRecord],
    orchestrators: &[OrchestratorRecord],
) -> Result<PortalSnapshot> {
    let cutoff = context.now.saturating_sub(SESSION_LOOKBACK_SECONDS);
    let source = FsArtifactSource::new(SESSION_ARTIFACT_MAX_BYTES);
    let discovery = discovery_context(context, runs, orchestrators);
    let mut records = Vec::new();
    for engine in Engine::ALL {
        let Some(provider) = runtime.catalog().providers.get(&engine) else {
            continue;
        };
        for profile in &provider.profiles {
            records.extend(discover_profile(
                &source,
                &context.paths.home,
                profile,
                &discovery,
                cutoff,
            )?);
        }
    }
    Ok(neomax_core::sessions::portal_snapshot(
        context.now,
        flatten_native_children(records),
    ))
}

fn discovery_context(
    context: &RuntimeContext,
    runs: &[RunRecord],
    orchestrators: &[OrchestratorRecord],
) -> DiscoveryContext {
    let dispatched_sessions = runs
        .iter()
        .filter_map(|run| run.session.clone())
        .collect::<BTreeSet<_>>();
    let mut worktrees = BTreeSet::from([context.paths.worktrees.clone()]);
    worktrees.extend(runs.iter().filter_map(|run| run.worktree.clone()));
    let orchestrator_sessions = orchestrators
        .iter()
        .map(|record| record.session.clone())
        .collect::<BTreeSet<_>>();
    let projects = context.project_registry();
    DiscoveryContext {
        now: context.now,
        active_window: 120,
        state_root: Some(context.paths.state.clone()),
        worktrees: worktrees.into_iter().collect(),
        dispatched_sessions,
        orchestrator_sessions,
        project_resolver: Some(Arc::new(move |path: &std::path::Path| {
            projects.project_of(path)
        })),
        ..DiscoveryContext::default()
    }
}

fn discover_profile<S: ArtifactSource>(
    source: &S,
    home: &std::path::Path,
    profile: &neomax_core::providers::catalog::ProfileSnapshot,
    context: &DiscoveryContext,
    cutoff: i64,
) -> CoreResult<Vec<neomax_core::sessions::SessionRecord>> {
    let account = profile.account.as_str();
    match profile.engine {
        Engine::Claude => {
            neomax_core::sessions::claude::discover(source, &profile.path, account, context, cutoff)
        }
        Engine::Codex => {
            neomax_core::sessions::codex::discover(source, &profile.path, account, context, cutoff)
        }
        Engine::Kimi => {
            neomax_core::sessions::kimi::discover(source, &profile.path, account, context, cutoff)
        }
        Engine::Grok => {
            neomax_core::sessions::grok::discover(source, &profile.path, account, context, cutoff)
        }
        Engine::Opencode => {
            let database = neomax_core::sessions::opencode::database_path(&profile.path, home);
            if !database.is_file() {
                return Ok(Vec::new());
            }
            neomax_core::sessions::opencode::database_records(&database, account, context, cutoff)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::providers::runtime::ProviderRuntime;

    #[test]
    fn empty_fixture_has_no_local_session_records() {
        let fixture = crate::tests::fixture();
        let snapshot = discover(&fixture.context, &ProviderRuntime::empty(), &[], &[]).unwrap();
        assert!(snapshot.sessions.is_empty());
        assert!(snapshot.subagents.is_empty());
    }
}
