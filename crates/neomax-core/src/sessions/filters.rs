use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Result;

use super::types::SessionRecord;

pub trait ProjectResolver: Send + Sync {
    fn project_of(&self, cwd: &Path) -> Option<String>;
}

impl<F> ProjectResolver for F
where
    F: Fn(&Path) -> Option<String> + Send + Sync,
{
    fn project_of(&self, cwd: &Path) -> Option<String> {
        self(cwd)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    StatePath,
    Worktree,
    DispatchedSession,
    InternalSession,
    OrchestratorSession,
    ParentExcluded,
}

#[derive(Clone)]
pub struct DiscoveryContext {
    pub now: i64,
    pub active_window: i64,
    pub state_root: Option<PathBuf>,
    pub worktrees: Vec<PathBuf>,
    pub dispatched_sessions: BTreeSet<String>,
    pub internal_sessions: BTreeSet<String>,
    pub orchestrator_sessions: BTreeSet<String>,
    pub project_resolver: Option<Arc<dyn ProjectResolver>>,
}

impl std::fmt::Debug for DiscoveryContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DiscoveryContext")
            .field("now", &self.now)
            .field("active_window", &self.active_window)
            .field("state_root", &self.state_root)
            .field("worktrees", &self.worktrees)
            .field("dispatched_sessions", &self.dispatched_sessions)
            .field("internal_sessions", &self.internal_sessions)
            .field("orchestrator_sessions", &self.orchestrator_sessions)
            .finish_non_exhaustive()
    }
}

impl Default for DiscoveryContext {
    fn default() -> Self {
        Self {
            now: 0,
            active_window: 120,
            state_root: None,
            worktrees: Vec::new(),
            dispatched_sessions: BTreeSet::new(),
            internal_sessions: BTreeSet::new(),
            orchestrator_sessions: BTreeSet::new(),
            project_resolver: None,
        }
    }
}

impl DiscoveryContext {
    pub fn new(now: i64) -> Self {
        Self {
            now,
            ..Self::default()
        }
    }

    pub fn with_state_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.state_root = Some(root.into());
        self
    }

    pub fn with_worktrees(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.worktrees = paths.into_iter().collect();
        self
    }

    pub fn with_project_resolver<R: ProjectResolver + 'static>(mut self, resolver: R) -> Self {
        self.project_resolver = Some(Arc::new(resolver));
        self
    }

    pub fn project_of(&self, cwd: Option<&Path>) -> Option<String> {
        self.project_resolver
            .as_ref()
            .and_then(|resolver| cwd.and_then(|path| resolver.project_of(path)))
    }

    pub fn exclusion_for(
        &self,
        id: &str,
        parent_id: Option<&str>,
        cwd: Option<&Path>,
    ) -> Option<ExclusionReason> {
        if parent_id.is_some_and(|parent| self.is_excluded_id(parent)) {
            return Some(ExclusionReason::ParentExcluded);
        }
        if self.is_excluded_id(id) {
            if self.dispatched_sessions.contains(id) {
                return Some(ExclusionReason::DispatchedSession);
            }
            if self.internal_sessions.contains(id) {
                return Some(ExclusionReason::InternalSession);
            }
            return Some(ExclusionReason::OrchestratorSession);
        }
        let cwd = cwd?;
        if self
            .state_root
            .as_deref()
            .is_some_and(|root| is_within(cwd, root))
        {
            return Some(ExclusionReason::StatePath);
        }
        if self.worktrees.iter().any(|root| is_within(cwd, root)) {
            return Some(ExclusionReason::Worktree);
        }
        None
    }

    pub fn include(&self, record: &SessionRecord) -> bool {
        self.exclusion_for(
            &record.id,
            record.parent_id.as_deref(),
            record.cwd.as_deref(),
        )
        .is_none()
    }

    fn is_excluded_id(&self, id: &str) -> bool {
        self.dispatched_sessions.contains(id)
            || self.internal_sessions.contains(id)
            || self.orchestrator_sessions.contains(id)
    }
}

fn is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub fn apply_context(record: &mut SessionRecord, context: &DiscoveryContext) -> Result<bool> {
    record.project = context.project_of(record.cwd.as_deref());
    record.worker = context.state_root.as_deref().is_some_and(|root| {
        record
            .cwd
            .as_deref()
            .is_some_and(|cwd| is_within(cwd, root))
    }) || context.worktrees.iter().any(|root| {
        record
            .cwd
            .as_deref()
            .is_some_and(|cwd| is_within(cwd, root))
    });
    record.orchestrator = context.orchestrator_sessions.contains(&record.id);
    record.update_age(context.now);
    Ok(context.include(record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;

    #[test]
    fn excludes_state_worktree_and_registered_worker_sessions() {
        let context = DiscoveryContext::new(100)
            .with_state_root("/state")
            .with_worktrees([PathBuf::from("/repo/worktrees/task")]);
        let mut state = SessionRecord::with_identity("state", Engine::Claude, "acct");
        state.cwd = Some(PathBuf::from("/state/logs"));
        assert!(!apply_context(&mut state, &context).unwrap());

        let mut worker = SessionRecord::with_identity("worker", Engine::Claude, "acct");
        worker.cwd = Some(PathBuf::from("/repo/worktrees/task/src"));
        assert!(!apply_context(&mut worker, &context).unwrap());

        let mut dispatched = SessionRecord::with_identity("run-session", Engine::Claude, "acct");
        dispatched.cwd = Some(PathBuf::from("/repo"));
        let context = DiscoveryContext {
            dispatched_sessions: BTreeSet::from(["run-session".into()]),
            ..context
        };
        assert!(!apply_context(&mut dispatched, &context).unwrap());
    }

    #[test]
    fn project_association_is_dependency_injected() {
        let context = DiscoveryContext::new(1).with_project_resolver(|path: &Path| {
            (path == Path::new("/repo")).then(|| "example".into())
        });
        let mut record = SessionRecord::with_identity("s", Engine::Codex, "acct");
        record.cwd = Some("/repo".into());
        assert!(apply_context(&mut record, &context).unwrap());
        assert_eq!(record.project.as_deref(), Some("example"));
    }
}
