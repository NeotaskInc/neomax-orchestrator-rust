use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use neomax_core::io::{LocalFileSource, ReadLimits, is_rooted_but_not_absolute, read_file};
use neomax_core::scheduler::{Plan, PlanSpec};
use neomax_core::{Error, WorkerScope};

use super::types::PlanRuntimeOptions;

#[derive(Debug, Clone)]
pub(crate) struct LoadedPlan {
    pub source: PathBuf,
    pub plan: Plan,
}

const PLAN_FILE_MAX_BYTES: usize = 4 * 1024 * 1024;
const PLAN_FILE_TIMEOUT: Duration = Duration::from_secs(5);
static NEXT_PLAN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlanAction {
    RunAll,
    Attach,
    Tick,
    Interrupt,
    Recover,
    Status,
    List,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanArguments {
    pub action: PlanAction,
    pub plan_id: Option<String>,
    pub plan_path: Option<PathBuf>,
    pub runtime: PlanRuntimeOptions,
    pub repository: Option<PathBuf>,
    pub base: Option<String>,
    pub integration_branch: Option<String>,
    pub error: Option<String>,
    pub scope: WorkerScope,
    pub wait: bool,
    pub json: bool,
}

impl Default for PlanArguments {
    fn default() -> Self {
        Self {
            action: PlanAction::Status,
            plan_id: None,
            plan_path: None,
            runtime: PlanRuntimeOptions::default(),
            repository: None,
            base: None,
            integration_branch: None,
            error: None,
            scope: WorkerScope::all(),
            wait: false,
            json: false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RunAllArguments {
    pub loaded: LoadedPlan,
    pub repository: PathBuf,
    pub base: Option<String>,
    pub integration_branch: Option<String>,
    pub plan_id: String,
    pub runtime: PlanRuntimeOptions,
}

#[derive(Debug, Clone)]
pub(crate) struct RunAllInput {
    pub path: PathBuf,
    pub cwd: PathBuf,
    pub scope: WorkerScope,
    pub runtime: PlanRuntimeOptions,
    pub repository: Option<PathBuf>,
    pub base: Option<String>,
    pub integration_branch: Option<String>,
    pub plan_id: Option<String>,
}

pub(crate) fn parse_action(action: PlanAction, args: &[String]) -> Result<PlanArguments> {
    let mut parsed = PlanArguments {
        action,
        ..PlanArguments::default()
    };
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let value = &args[index];
        if let Some((name, flag_value)) = value.split_once('=') {
            if name.starts_with('-') {
                set_flag(&mut parsed, name, flag_value)?;
                index += 1;
                continue;
            }
        }
        if value.starts_with('-') {
            let (name, consumed) = match value.as_str() {
                "--json" | "--wait" => (value.as_str(), false),
                "--repo"
                | "--base"
                | "--integration-branch"
                | "--plan-id"
                | "--error"
                | "--workers"
                | "--max-live"
                | "--max-stall-cycles"
                | "--max-attempts"
                | "--max-ticks" => (value.as_str(), true),
                _ => return Err(anyhow::anyhow!("unknown scheduler option {value}")),
            };
            if consumed {
                index += 1;
                let next = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("scheduler option {name} requires a value"))?;
                set_flag(&mut parsed, name, next)?;
            } else {
                set_flag(&mut parsed, name, "true")?;
            }
            index += 1;
            continue;
        }
        positionals.push(value.clone());
        index += 1;
    }
    match action {
        PlanAction::RunAll => {
            let path = positionals
                .first()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("run-all requires a plan JSON path"))?;
            parsed.plan_path = Some(path);
        }
        PlanAction::Status | PlanAction::List => {
            if positionals.len() > 1 {
                bail!("scheduler status accepts at most one plan id")
            }
            parsed.plan_id = positionals.first().cloned();
        }
        PlanAction::Attach | PlanAction::Tick | PlanAction::Interrupt | PlanAction::Recover => {
            let plan_id = positionals
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("scheduler command requires a plan id"))?;
            if positionals.len() > 1 {
                bail!("scheduler command accepts exactly one plan id")
            }
            parsed.plan_id = Some(plan_id);
        }
    }
    parsed.runtime.validate()?;
    Ok(parsed)
}

pub(crate) fn load_run_all(
    args: &RunAllArguments,
    _cwd: &Path,
    scope: &WorkerScope,
) -> neomax_core::Result<neomax_core::scheduler::service::RunAllSpec> {
    require_absolute_path(&args.repository, "scheduler repository")?;
    let mut plan = args.loaded.plan.clone();
    let repository = args.repository.clone();
    plan.repo = Some(repository.clone());
    plan.base = args.base.clone().or(plan.base);
    plan.integration_branch = args.integration_branch.clone().or(plan.integration_branch);
    let base = plan.base.clone();
    let integration_branch = plan.integration_branch.clone();
    let plan_id = args.plan_id.clone();
    plan.plan_id = Some(plan_id.clone());
    if !scope
        .engines()
        .any(|engine| plan.parts.iter().any(|part| part.engine == engine))
    {
        return Err(Error::InvalidArgument(format!(
            "scheduler plan {plan_id} has no part in worker scope {}",
            scope.csv()
        )));
    }
    plan.graph()?;
    Ok(neomax_core::scheduler::service::RunAllSpec {
        plan,
        repository,
        base,
        integration_branch,
        plan_id,
        runtime: args.runtime.runtime,
    })
}

pub(crate) fn load_plan(
    path: &Path,
    cwd: &Path,
    scope: &WorkerScope,
) -> neomax_core::Result<LoadedPlan> {
    let path = resolve_plan_path(path, cwd)?;
    let limits = ReadLimits::new(PLAN_FILE_MAX_BYTES, PLAN_FILE_TIMEOUT).map_err(Error::from)?;
    let bytes = read_file(&LocalFileSource, &path, limits).map_err(Error::from)?;
    let spec: PlanSpec = serde_json::from_slice(&bytes).map_err(|error| {
        Error::InvalidArgument(format!("neomax run-all: cannot parse plan: {error}"))
    })?;
    let plan = Plan::normalize(spec, scope)?;
    Ok(LoadedPlan { source: path, plan })
}

pub(crate) fn normalize_run_all(input: RunAllInput) -> neomax_core::Result<RunAllArguments> {
    input.runtime.validate()?;
    require_absolute_path(&input.cwd, "scheduler working directory")?;
    let loaded = load_plan(&input.path, &input.cwd, &input.scope)?;
    let repository = input.repository.or_else(|| loaded.plan.repo.clone());
    let repository = repository
        .map(|value| resolve_repository_path(&value, &input.cwd))
        .transpose()?;
    let plan_id = input
        .plan_id
        .or_else(|| loaded.plan.plan_id.clone())
        .unwrap_or_else(|| default_plan_id(&loaded.source));
    neomax_core::scheduler::persistence::validate_plan_id(&plan_id)?;
    Ok(RunAllArguments {
        loaded,
        repository: repository.unwrap_or(input.cwd),
        base: input.base,
        integration_branch: input.integration_branch,
        plan_id,
        runtime: input.runtime,
    })
}

fn resolve_plan_path(path: &Path, cwd: &Path) -> neomax_core::Result<PathBuf> {
    require_absolute_path(cwd, "scheduler working directory")?;
    reject_unsafe_path(path, "plan path")?;
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    })
}

fn resolve_repository_path(path: &Path, cwd: &Path) -> neomax_core::Result<PathBuf> {
    reject_unsafe_path(path, "scheduler repository")?;
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    require_absolute_path(&resolved, "scheduler repository")?;
    Ok(resolved)
}

fn require_absolute_path(path: &Path, label: &str) -> neomax_core::Result<()> {
    reject_unsafe_path(path, label)?;
    if !path.is_absolute() {
        return Err(Error::InvalidArgument(format!(
            "{label} must be absolute: {}",
            path.display()
        )));
    }
    Ok(())
}

fn reject_unsafe_path(path: &Path, label: &str) -> neomax_core::Result<()> {
    if is_rooted_but_not_absolute(path) {
        return Err(Error::InvalidArgument(format!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidArgument(format!(
            "{label} cannot contain parent-directory traversal: {}",
            path.display()
        )));
    }
    Ok(())
}

fn set_flag(parsed: &mut PlanArguments, name: &str, value: &str) -> Result<()> {
    match name {
        "--json" => parsed.json = true,
        "--wait" => parsed.wait = true,
        "--repo" => {
            parsed.repository = Some(PathBuf::from(value));
        }
        "--base" => {
            parsed.base = Some(value.to_owned());
        }
        "--integration-branch" => {
            parsed.integration_branch = Some(value.to_owned());
        }
        "--plan-id" => {
            parsed.plan_id = Some(value.to_owned());
        }
        "--error" => {
            parsed.error = Some(value.to_owned());
        }
        "--workers" => {
            parsed.scope = value
                .parse()
                .map_err(|error| anyhow::anyhow!("invalid --workers: {error}"))?;
        }
        "--max-live" => {
            parsed.runtime.runtime.max_live = parse_positive(value, name)?;
            parsed.runtime.max_live_explicit = true;
        }
        "--max-stall-cycles" => {
            parsed.runtime.runtime.max_stall_cycles = parse_positive(value, name)?
        }
        "--max-attempts" => parsed.runtime.runtime.max_attempts = parse_positive(value, name)?,
        "--max-ticks" => parsed.runtime.max_ticks = parse_positive(value, name)?,
        _ => bail!("unknown scheduler option {name}"),
    }
    Ok(())
}

fn parse_positive<T>(value: &str, name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let parsed = value
        .parse::<T>()
        .map_err(|error| anyhow::anyhow!("{name} must be a positive integer: {error}"))?;
    Ok(parsed)
}

fn default_plan_id(path: &Path) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_PLAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    default_plan_id_with_identity(path, timestamp, std::process::id(), sequence)
}

pub(super) fn default_plan_id_with_identity(
    path: &Path,
    timestamp: u128,
    pid: u32,
    sequence: u64,
) -> String {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("plan");
    let suffix = stem
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | '-') {
                value
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stem = suffix.trim_matches('.');
    let stem = if stem.is_empty() { "plan" } else { stem };
    let identity = format!("-{timestamp}-{pid}-{sequence}");
    let available = 128usize.saturating_sub("plan-".len() + identity.len());
    let stem = &stem[..stem.len().min(available)];
    if stem.is_empty() {
        format!("plan{identity}")
    } else {
        format!("plan-{stem}{identity}")
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::fs;

    fn scope() -> WorkerScope {
        "opencode".parse().expect("worker scope")
    }

    #[test]
    fn plan_paths_reject_parent_traversal_before_resolving() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        let error = load_plan(Path::new("../plan.json"), temp.path(), &scope()).unwrap_err();
        assert!(error.to_string().contains("parent-directory traversal"));
    }

    #[test]
    fn relative_repository_labels_stay_below_the_absolute_working_directory() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        let plan_path = temp.path().join("plan.json");
        fs::write(
            &plan_path,
            br#"{"parts":[{"id":"one","prompt":"inspect","engine":"opencode"}]}"#,
        )
        .expect("plan fixture");
        let arguments = normalize_run_all(RunAllInput {
            path: plan_path,
            cwd: temp.path().to_path_buf(),
            scope: scope(),
            runtime: Default::default(),
            repository: Some(PathBuf::from("repo")),
            base: None,
            integration_branch: None,
            plan_id: Some("plan".into()),
        })
        .expect("normalize plan");
        assert_eq!(arguments.repository, temp.path().join("repo"));
    }

    #[test]
    fn persisted_repository_traversal_is_rejected_before_joining() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        let plan_path = temp.path().join("plan.json");
        fs::write(
            &plan_path,
            br#"{"repo":"../outside","parts":[{"id":"one","prompt":"inspect","engine":"opencode"}]}"#,
        )
        .expect("plan fixture");
        let error = normalize_run_all(RunAllInput {
            path: plan_path,
            cwd: temp.path().to_path_buf(),
            scope: scope(),
            runtime: Default::default(),
            repository: None,
            base: None,
            integration_branch: None,
            plan_id: Some("plan".into()),
        })
        .unwrap_err();
        assert!(error.to_string().contains("parent-directory traversal"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_partial_roots_are_rejected_before_resolution() {
        let temp = tempfile::tempdir().expect("temporary workspace");
        for value in [r"\plan.json", r"C:plan.json"] {
            let error = load_plan(Path::new(value), temp.path(), &scope()).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
    }
}
