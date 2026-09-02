use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::runtime::{RuntimeEnvironment, resolve_path};
use crate::{Error, Result};

pub use crate::providers::catalog::ModelDefaults;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Claude,
    Codex,
    Opencode,
    Kimi,
    Grok,
}

impl Engine {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::Opencode,
        Self::Kimi,
        Self::Grok,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Kimi => "kimi",
            Self::Grok => "grok",
        }
    }
}

impl Display for Engine {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Engine {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "opencode" | "open-code" => Ok(Self::Opencode),
            "kimi" => Ok(Self::Kimi),
            "grok" => Ok(Self::Grok),
            other => Err(Error::InvalidArgument(format!("unknown engine {other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerScope(BTreeSet<Engine>);

impl WorkerScope {
    pub fn all() -> Self {
        Self(Engine::ALL.into_iter().collect())
    }

    pub fn only(engine: Engine) -> Self {
        Self(BTreeSet::from([engine]))
    }

    pub fn contains(&self, engine: Engine) -> bool {
        self.0.contains(&engine)
    }

    pub fn intersection(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0).copied().collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn engines(&self) -> impl Iterator<Item = Engine> + '_ {
        self.0.iter().copied()
    }

    pub fn csv(&self) -> String {
        self.engines()
            .map(Engine::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl FromStr for WorkerScope {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        if value.trim().eq_ignore_ascii_case("all") {
            return Ok(Self::all());
        }
        let engines = value
            .split([',', '+'])
            .filter(|item| !item.trim().is_empty())
            .map(Engine::from_str)
            .collect::<Result<BTreeSet<_>>>()?;
        if engines.is_empty() {
            return Err(Error::InvalidArgument("worker scope is empty".into()));
        }
        Ok(Self(engines))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePaths {
    pub home: PathBuf,
    pub state: PathBuf,
    pub runs: PathBuf,
    pub logs: PathBuf,
    /// Legacy mixed event root retained for readers and migration.
    pub events: PathBuf,
    pub run_events: PathBuf,
    pub issue_events: PathBuf,
    pub scheduler_events: PathBuf,
    pub usage: PathBuf,
    pub usage_ledger: PathBuf,
    pub usage_watch: PathBuf,
    pub worktrees: PathBuf,
    pub orchestrators: PathBuf,
    pub orchestrator_selection: PathBuf,
    pub cooldowns: PathBuf,
    pub account_cooldown: PathBuf,
    pub account_cooldown_lock: PathBuf,
    pub paused: PathBuf,
    pub armed_rotate: PathBuf,
    pub armed_rotate_lock: PathBuf,
    pub rotation_claims: PathBuf,
    pub rotation_lock: PathBuf,
    pub auth_backups: PathBuf,
    pub auth_rotations: PathBuf,
    pub history_db: PathBuf,
    pub history_logs: PathBuf,
    pub history_pending: PathBuf,
    pub projects: PathBuf,
    pub tasks: PathBuf,
    pub agent_queue: PathBuf,
    pub plans: PathBuf,
    pub area_locks: PathBuf,
    pub self_heal: PathBuf,
    pub self_heal_lock: PathBuf,
}

impl StatePaths {
    pub fn discover() -> Result<Self> {
        Self::discover_from(&RuntimeEnvironment::process())
    }

    pub fn discover_from(runtime: &RuntimeEnvironment) -> Result<Self> {
        let home = runtime
            .home_dir()
            .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))?;
        require_absolute_root("HOME", &home, runtime.platform())?;
        let state = match runtime.value("NEOMAX_HOME") {
            Some(value) => {
                let state = PathBuf::from(value);
                require_absolute_root("NEOMAX_HOME", &state, runtime.platform())?;
                state
            }
            None => home.join(".neomax"),
        };
        Ok(Self::new(home, state))
    }

    pub fn new(home: impl Into<PathBuf>, state: impl Into<PathBuf>) -> Self {
        let home = home.into();
        let state = state.into();
        let events = state.join("events");
        Self {
            runs: state.join("runs"),
            logs: state.join("logs"),
            events: events.clone(),
            run_events: events.join("runs"),
            issue_events: events.join("issues"),
            scheduler_events: events.join("scheduler"),
            usage: state.join("usage"),
            usage_ledger: state.join("usage-ledger"),
            usage_watch: state.join("usage-watch.state.json"),
            worktrees: state.join("worktrees"),
            orchestrators: state.join("orchestrators"),
            orchestrator_selection: state.join("neomax-selection.json"),
            cooldowns: state.join("cooldown.json"),
            account_cooldown: state.join("account-cooldown.json"),
            account_cooldown_lock: state.join("account-cooldown.lock"),
            paused: state.join("paused.json"),
            armed_rotate: state.join("armed-rotate.json"),
            armed_rotate_lock: state.join("armed-rotate.lock"),
            rotation_claims: state.join("rotation-claims.json"),
            rotation_lock: state.join("rotation.lock"),
            auth_backups: state.join("auth-backups"),
            auth_rotations: state.join("auth-rotations.jsonl"),
            history_db: state.join("history.db"),
            history_logs: state.join("history-logs"),
            history_pending: state.join("history-pending"),
            projects: state.join("projects.json"),
            tasks: state.join("tasks.json"),
            agent_queue: state.join("agent-queue.json"),
            plans: state.join("plans"),
            area_locks: state.join("locks"),
            self_heal: state.join("self-heal.json"),
            self_heal_lock: state.join("self-heal.lock"),
            home,
            state,
        }
    }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        // Keep the complete state directory tree pinned while it is being
        // created.  On Windows this prevents a checked component from being
        // replaced by a junction before the next component is traversed.
        let mut guards = Vec::new();
        for path in [
            &self.state,
            &self.runs,
            &self.logs,
            &self.events,
            &self.run_events,
            &self.issue_events,
            &self.scheduler_events,
            &self.usage,
            &self.usage_ledger,
            &self.worktrees,
            &self.orchestrators,
            &self.plans,
            &self.area_locks,
        ] {
            guards.push(crate::io::PathGuard::ensure_directory(path)?);
        }
        Ok(())
    }

    pub fn relative_to_home<'a>(&'a self, path: &'a Path) -> Option<&'a Path> {
        path.strip_prefix(&self.home).ok()
    }

    pub fn resolve_path(&self, value: &str) -> PathBuf {
        resolve_path(
            value,
            Some(&self.home),
            &self.home,
            crate::runtime::RuntimePlatform::current(),
        )
    }
}

fn require_absolute_root(
    label: &str,
    path: &Path,
    platform: crate::runtime::RuntimePlatform,
) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(format!(
            "{label} must be an absolute path"
        )));
    }
    if rooted_without_absolute(path, platform) {
        return Err(Error::InvalidArgument(format!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    if !absolute_for_platform(path, platform) {
        return Err(Error::InvalidArgument(format!(
            "{label} must be an absolute path: {}",
            path.display()
        )));
    }
    Ok(())
}

fn rooted_without_absolute(path: &Path, platform: crate::runtime::RuntimePlatform) -> bool {
    crate::io::is_rooted_but_not_absolute(path)
        || (platform.is_windows() && {
            let raw = path.to_string_lossy();
            let bytes = raw.as_bytes();
            (raw.starts_with(['\\', '/'])
                && !raw.starts_with("\\\\")
                && !raw.starts_with("//"))
                || (bytes.get(1) == Some(&b':')
                    && !bytes
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))
        })
}

fn absolute_for_platform(path: &Path, platform: crate::runtime::RuntimePlatform) -> bool {
    path.is_absolute()
        || (platform.is_windows() && {
            let raw = path.to_string_lossy();
            let bytes = raw.as_bytes();
            raw.starts_with("\\\\")
                || raw.starts_with("//")
                || (bytes.get(1) == Some(&b':')
                    && bytes
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_worker_scope_form() {
        let mixed: WorkerScope = "codex+opencode,kimi".parse().unwrap();
        assert!(mixed.contains(Engine::Codex));
        assert!(mixed.contains(Engine::Opencode));
        assert!(mixed.contains(Engine::Kimi));
        assert!(!mixed.contains(Engine::Claude));
        assert_eq!(WorkerScope::all().engines().count(), 5);
    }

    #[test]
    fn intersections_are_used_to_enforce_an_inherited_fleet() {
        let requested: WorkerScope = "claude,codex".parse().unwrap();
        let inherited: WorkerScope = "codex,opencode".parse().unwrap();
        let effective = requested.intersection(&inherited);
        assert_eq!(effective.csv(), "codex");
        assert!(!effective.is_empty());
        assert!(requested.intersection(&"grok".parse().unwrap()).is_empty());
    }

    #[test]
    fn defaults_match_the_release_contract() {
        let models = ModelDefaults::default();
        assert_eq!(models.for_engine(Engine::Claude), "claude-fable-5[1m]");
        assert_eq!(models.for_engine(Engine::Codex), "gpt-5.6-sol");
        assert_eq!(
            models.for_engine(Engine::Opencode),
            "opencode/big-pickle"
        );
        assert_eq!(models.for_engine(Engine::Kimi), "kimi-code/k3");
        assert_eq!(models.for_engine(Engine::Grok), "grok-4.6");
    }

    #[test]
    fn state_paths_remain_compatible_with_existing_installations() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        let paths = StatePaths::new(temp.path().join("home"), &state);
        assert_eq!(paths.history_db, state.join("history.db"));
        assert_eq!(paths.events, state.join("events"));
        assert_eq!(paths.run_events, state.join("events/runs"));
        assert_eq!(paths.issue_events, state.join("events/issues"));
        assert_eq!(paths.scheduler_events, state.join("events/scheduler"));
        assert_eq!(paths.usage_ledger, state.join("usage-ledger"));
        assert_eq!(paths.usage_watch, state.join("usage-watch.state.json"));
        assert_eq!(paths.history_pending, state.join("history-pending"));
        assert_eq!(
            paths.orchestrator_selection,
            state.join("neomax-selection.json")
        );
        assert_eq!(paths.projects, state.join("projects.json"));
        assert_eq!(paths.tasks, state.join("tasks.json"));
        assert_eq!(paths.agent_queue, state.join("agent-queue.json"));
        assert_eq!(paths.plans, state.join("plans"));
        assert_eq!(paths.area_locks, state.join("locks"));
        assert_eq!(paths.self_heal, state.join("self-heal.json"));
        assert_eq!(paths.self_heal_lock, state.join("self-heal.lock"));
        assert_eq!(paths.account_cooldown, state.join("account-cooldown.json"));
        assert_eq!(
            paths.account_cooldown_lock,
            state.join("account-cooldown.lock")
        );
        assert_eq!(paths.armed_rotate, state.join("armed-rotate.json"));
        assert_eq!(paths.armed_rotate_lock, state.join("armed-rotate.lock"));
        assert_eq!(paths.auth_backups, state.join("auth-backups"));
        assert_eq!(paths.auth_rotations, state.join("auth-rotations.jsonl"));
    }

    #[test]
    fn explicit_state_roots_require_absolute_paths() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let relative = RuntimeEnvironment::fixture(
            crate::runtime::RuntimePlatform::Unix,
            [
                ("HOME".into(), home.to_string_lossy().into_owned()),
                ("NEOMAX_HOME".into(), "relative-state".into()),
            ],
            temp.path(),
        );
        let error = StatePaths::discover_from(&relative).unwrap_err();
        assert!(error.to_string().contains("NEOMAX_HOME"));
        assert!(error.to_string().contains("absolute"));
    }

    #[test]
    fn explicit_state_roots_preserve_absolute_paths() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let state = temp.path().join("state");
        let runtime = RuntimeEnvironment::fixture(
            crate::runtime::RuntimePlatform::Unix,
            [
                ("HOME".into(), home.to_string_lossy().into_owned()),
                ("NEOMAX_HOME".into(), state.to_string_lossy().into_owned()),
            ],
            temp.path(),
        );
        let paths = StatePaths::discover_from(&runtime).unwrap();
        assert_eq!(paths.home, home);
        assert_eq!(paths.state, state);
    }

    #[test]
    fn windows_absolute_state_roots_are_portable_injected_values() {
        let runtime = RuntimeEnvironment::fixture(
            crate::runtime::RuntimePlatform::Windows,
            [
                ("USERPROFILE".into(), r"C:\Users\fixture".into()),
                ("NEOMAX_HOME".into(), r"D:\Neomax\state".into()),
            ],
            r"C:\work",
        );
        let paths = StatePaths::discover_from(&runtime).unwrap();
        assert_eq!(paths.home, PathBuf::from(r"C:\Users\fixture"));
        assert_eq!(paths.state, PathBuf::from(r"D:\Neomax\state"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_partial_state_roots_fail_closed() {
        for value in [r"\rooted", r"C:drive-relative"] {
            let runtime = RuntimeEnvironment::fixture(
                crate::runtime::RuntimePlatform::Windows,
                [
                    ("USERPROFILE".into(), r"C:\Users\fixture".into()),
                    ("NEOMAX_HOME".into(), value.into()),
                ],
                r"C:\work",
            );
            assert!(StatePaths::discover_from(&runtime).is_err());
        }
    }
}
