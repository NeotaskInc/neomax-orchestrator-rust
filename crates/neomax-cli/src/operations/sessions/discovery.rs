use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::registry::OrchestratorStore;
use neomax_core::providers::catalog::ProfileSnapshot;
use neomax_core::runs::{RunStore, SystemProcessProbe};
use neomax_core::sessions::artifacts::{ArtifactSource, FsArtifactSource};
use neomax_core::sessions::filters::DiscoveryContext;
use neomax_core::sessions::{SessionRecord, flatten_native_children};
use neomax_core::{Engine, Result as CoreResult};

use crate::context::RuntimeContext;

use super::filters::SessionFilters;

struct ProjectLookup {
    roots: Vec<(String, PathBuf, Vec<PathBuf>)>,
}

impl ProjectLookup {
    fn from_context(context: &RuntimeContext) -> Self {
        let roots = context
            .project_registry()
            .load()
            .into_iter()
            .filter_map(|(name, project)| {
                let root = normalize_absolute_path(&project.root)?;
                let repos = project
                    .repos
                    .into_iter()
                    .filter_map(|repo| {
                        reject_unsafe_path(&repo).ok()?;
                        let path = if repo.is_absolute() {
                            repo
                        } else {
                            root.join(repo)
                        };
                        normalize_absolute_path(&path)
                    })
                    .collect();
                Some((name, root, repos))
            })
            .collect();
        Self { roots }
    }

    fn project_of(&self, path: &Path) -> Option<String> {
        let path = normalize_absolute_path(path)?;
        self.roots
            .iter()
            .filter(|(_, root, _)| path == root.as_path() || path.starts_with(root))
            .max_by_key(|(_, root, _)| root.components().count())
            .map(|(name, _, _)| name.clone())
            .or_else(|| {
                let name = path.file_name()?;
                let matches = self.roots.iter().filter(|(_, _, repos)| {
                    repos.iter().any(|repo| repo.file_name() == Some(name))
                });
                let mut matches = matches.map(|(name, _, _)| name);
                let owner = matches.next()?;
                matches.next().is_none().then(|| owner.clone())
            })
    }
}

fn normalize_absolute_path(path: &Path) -> Option<PathBuf> {
    reject_unsafe_path(path).ok()?;
    if !path.is_absolute() {
        return None;
    }
    let mut existing = path.to_path_buf();
    let mut suffix = Vec::new();
    while !existing.exists() {
        suffix.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?.to_path_buf();
    }
    let mut resolved = existing.canonicalize().ok()?;
    for component in suffix.iter().rev() {
        resolved.push(component);
    }
    Some(resolved)
}

fn reject_unsafe_path(path: &Path) -> CoreResult<()> {
    if is_rooted_but_not_absolute(path) {
        return Err(neomax_core::Error::InvalidArgument(format!(
            "path must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(neomax_core::Error::InvalidArgument(format!(
            "path cannot contain parent-directory traversal: {}",
            path.display()
        )));
    }
    Ok(())
}

const ARTIFACT_MAX_BYTES: usize = 128 * 1024 * 1024;
const SECONDS_PER_DAY: i64 = 86_400;
pub(crate) const MAX_DISCOVERY_DAYS: u32 = 3_650;

#[derive(Debug, Clone)]
pub(crate) struct DiscoveryOptions {
    pub days: u32,
    pub limit: usize,
    pub filters: SessionFilters,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            days: 3,
            limit: 60,
            filters: SessionFilters::default(),
        }
    }
}

impl DiscoveryOptions {
    pub(crate) fn cutoff(&self, now: i64) -> i64 {
        if self.days > MAX_DISCOVERY_DAYS {
            return i64::MIN;
        }
        now.saturating_sub(i64::from(self.days).saturating_mul(SECONDS_PER_DAY))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SessionInventory {
    pub all_records: Vec<SessionRecord>,
    pub(crate) owners: BTreeMap<SessionKey, PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SessionKey {
    pub engine: Engine,
    pub account: String,
    pub id: String,
}

impl SessionKey {
    pub(crate) fn for_record(record: &SessionRecord) -> Self {
        Self {
            engine: record.engine,
            account: record.account.clone(),
            id: record.id.clone(),
        }
    }
}

impl SessionInventory {
    pub(crate) fn records(
        &self,
        subagents: bool,
        options: &DiscoveryOptions,
    ) -> Vec<SessionRecord> {
        let mut records = self
            .all_records
            .iter()
            .filter(|record| record.is_child() == subagents)
            .filter(|record| options.filters.matches(record))
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            right
                .last_active
                .unwrap_or_default()
                .cmp(&left.last_active.unwrap_or_default())
                .then_with(|| left.engine.cmp(&right.engine))
                .then_with(|| left.account.cmp(&right.account))
                .then_with(|| left.id.cmp(&right.id))
        });
        records.truncate(options.limit);
        records
    }
}

pub(crate) fn discover(
    context: &RuntimeContext,
    options: &DiscoveryOptions,
) -> Result<SessionInventory> {
    let home = normalize_absolute_path(&context.paths.home)
        .ok_or_else(|| anyhow::anyhow!("Neomax home path must be absolute and safe"))?;
    let runtime = context.provider_runtime()?;
    let discovery_context = discovery_context(context)?;
    let cutoff = options.cutoff(context.now);
    let source = FsArtifactSource::new(ARTIFACT_MAX_BYTES);
    let mut records = Vec::new();
    let mut owners = BTreeMap::new();
    for engine in Engine::ALL {
        let Some(provider) = runtime.catalog().providers.get(&engine) else {
            continue;
        };
        for profile in &provider.profiles {
            let profile_records =
                discover_profile(&source, &home, profile, &discovery_context, cutoff)?;
            for record in &profile_records {
                record_owner(record, &profile.path, &mut owners);
            }
            records.extend(profile_records);
        }
    }
    let all_records = flatten_native_children(records);
    Ok(SessionInventory {
        all_records,
        owners,
    })
}

fn record_owner(
    record: &SessionRecord,
    profile: &Path,
    owners: &mut BTreeMap<SessionKey, PathBuf>,
) {
    owners.insert(SessionKey::for_record(record), profile.to_path_buf());
    for child in &record.children {
        record_owner(child, profile, owners);
    }
}

impl SessionInventory {
    pub(crate) fn owner(&self, record: &SessionRecord) -> Option<&Path> {
        self.owners
            .get(&SessionKey::for_record(record))
            .map(PathBuf::as_path)
    }
}

fn discover_profile<S: ArtifactSource>(
    source: &S,
    home: &Path,
    profile: &ProfileSnapshot,
    context: &DiscoveryContext,
    cutoff: i64,
) -> CoreResult<Vec<SessionRecord>> {
    if normalize_absolute_path(&profile.path).is_none() {
        return Ok(Vec::new());
    }
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
            let path = neomax_core::sessions::opencode::database_path(&profile.path, home);
            if !path.is_file() {
                return Ok(Vec::new());
            }
            neomax_core::sessions::opencode::database_records(&path, account, context, cutoff)
        }
    }
}

fn discovery_context(context: &RuntimeContext) -> Result<DiscoveryContext> {
    let run_store = RunStore::new(&context.paths.runs);
    let runs = run_store.all()?;
    let dispatched_sessions = runs
        .iter()
        .filter_map(|run| run.session.clone())
        .collect::<BTreeSet<_>>();
    let mut worktrees = BTreeSet::new();
    if let Some(root) = normalize_absolute_path(&context.paths.worktrees) {
        worktrees.insert(root);
    }
    worktrees.extend(
        runs.iter()
            .filter_map(|run| run.worktree.as_deref().and_then(normalize_absolute_path)),
    );
    let orchestrator_store = OrchestratorStore::new(&context.paths.orchestrators);
    let orchestrators = orchestrator_store.all(&SystemProcessProbe, context.now)?;
    let orchestrator_sessions = orchestrators
        .iter()
        .map(|record| record.session.clone())
        .collect::<BTreeSet<_>>();
    let projects = ProjectLookup::from_context(context);
    Ok(DiscoveryContext {
        now: context.now,
        state_root: normalize_absolute_path(&context.paths.state),
        worktrees: worktrees.into_iter().collect(),
        dispatched_sessions,
        orchestrator_sessions,
        project_resolver: Some(Arc::new(move |path: &Path| projects.project_of(path))),
        ..DiscoveryContext::default()
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use neomax_core::sessions::artifacts::{ArtifactKind, MemoryArtifactSource, artifact};
    use neomax_core::sessions::filters::DiscoveryContext;

    #[test]
    fn persisted_project_paths_require_absolute_non_traversing_roots() {
        let temp = tempfile::tempdir().expect("temporary root");
        assert!(normalize_absolute_path(temp.path()).is_some());
        assert!(normalize_absolute_path(Path::new("../project")).is_none());
        assert!(normalize_absolute_path(Path::new("project")).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn persisted_project_paths_reject_windows_partial_roots() {
        assert!(normalize_absolute_path(Path::new(r"\project")).is_none());
        assert!(normalize_absolute_path(Path::new(r"C:project")).is_none());
    }

    #[test]
    fn project_lookup_keeps_safe_relative_repository_labels() {
        let temp = tempfile::tempdir().expect("temporary root");
        let root = temp.path().join("project");
        let repository = root.join("nested/repository");
        std::fs::create_dir_all(&repository).expect("repository");
        let lookup = ProjectLookup {
            roots: vec![(
                "project".into(),
                normalize_absolute_path(&root).expect("root"),
                vec![normalize_absolute_path(&repository).expect("repository")],
            )],
        };
        assert_eq!(
            lookup.project_of(&repository.join("src")).as_deref(),
            Some("project")
        );
    }
    #[test]
    fn options_cutoff_saturates_at_the_oldest_timestamp() {
        let options = DiscoveryOptions {
            days: u32::MAX,
            ..DiscoveryOptions::default()
        };
        assert_eq!(options.cutoff(10), i64::MIN);
    }

    #[test]
    fn each_file_backed_provider_uses_the_core_discovery_owner() {
        let temp = tempfile::tempdir().expect("temporary root");
        let profile = temp.path().join("profile");
        let home = temp.path().join("home");
        let repository = temp
            .path()
            .join("repository")
            .to_string_lossy()
            .into_owned();
        let source = MemoryArtifactSource::new([
            artifact(
                &profile,
                profile.join("projects/example/main.jsonl"),
                ArtifactKind::ClaudeMain,
                100,
                serde_json::json!({
                    "sessionId": "claude-main",
                    "cwd": repository.clone(),
                    "timestamp": 100,
                })
                .to_string()
                .into_bytes(),
            ),
            artifact(
                &profile,
                profile.join("sessions/2026/rollout-abc.jsonl"),
                ArtifactKind::CodexRollout,
                100,
                serde_json::json!({
                    "type": "session_meta",
                    "payload": {"id": "codex-main", "cwd": repository.clone()},
                })
                .to_string()
                .into_bytes(),
            ),
            artifact(
                &profile,
                profile.join("sessions/kimi/state.json"),
                ArtifactKind::KimiState,
                100,
                serde_json::json!({
                    "sessionId": "kimi-main",
                    "workDir": repository.clone(),
                    "agents": {"main": {"type": "main"}},
                })
                .to_string()
                .into_bytes(),
            ),
            artifact(
                &profile,
                profile.join("sessions/grok/summary.json"),
                ArtifactKind::GrokSummary,
                100,
                serde_json::json!({"info": {"id": "grok-main", "cwd": repository}})
                    .to_string()
                    .into_bytes(),
            ),
        ]);
        let context = DiscoveryContext::new(100);
        let profiles = [
            (Engine::Claude, "claude"),
            (Engine::Codex, "codex"),
            (Engine::Kimi, "kimi"),
            (Engine::Grok, "grok"),
        ];
        for (engine, account) in profiles {
            let snapshot = ProfileSnapshot {
                engine,
                account: account.into(),
                path: profile.clone(),
                reserved: false,
                auth: neomax_core::providers::catalog::AuthStatus::Unknown,
                eligibility: neomax_core::providers::catalog::ProfileEligibility::disconnected(),
            };
            let rows = discover_profile(&source, &home, &snapshot, &context, 0).unwrap();
            assert!(
                !rows.is_empty(),
                "{engine} should be discovered by its core owner"
            );
        }
    }

    #[test]
    fn opencode_uses_the_canonical_snapshot_owner_for_native_children() {
        let temp = tempfile::tempdir().expect("temporary root");
        let repository = temp
            .path()
            .join("repository")
            .to_string_lossy()
            .into_owned();
        let snapshot = serde_json::json!({
            "sessions": [
                {"id":"main","cwd":repository.clone(),"active":true},
                {"id":"child","parent_id":"main","cwd":repository,"active":true}
            ]
        });
        let rows = neomax_core::sessions::opencode::discover_snapshot(
            &snapshot,
            "acct",
            &DiscoveryContext::new(100),
        );
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(SessionRecord::is_child));
    }
}
