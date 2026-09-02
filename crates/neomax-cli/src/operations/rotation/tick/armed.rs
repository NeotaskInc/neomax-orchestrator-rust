use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use neomax_core::accounts::{
    AccountControlStore, AccountInventory, AccountSnapshot, RotationClaimStore,
};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::handoff::{HandoffTargetRequest, TargetPolicy, select_target};
use neomax_core::orchestration::registry::{OrchestratorRecord, OrchestratorStore};
use neomax_core::orchestration::rotation::{
    ArmedRotateRecord, ArmedRotateStore, engine_has_five_hour, normalize_profile_path,
};
use neomax_core::runs::{RunLiveWorkSource, RunStore, SystemProcessProbe};
use neomax_core::usage::UsageCacheStore;
use neomax_core::{Engine, WorkerScope};
use serde::Serialize;

use crate::context::RuntimeContext;
use crate::launch;
use crate::operations::handoff;
use crate::parser;

const ACTIVE_GRACE_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ArmedRotationReport {
    pub(crate) profile: PathBuf,
    pub(crate) engine: String,
    pub(crate) session: Option<String>,
    pub(crate) status: String,
}

#[derive(Debug, Default)]
pub(crate) struct SweepResult {
    pub(crate) reports: Vec<ArmedRotationReport>,
    pub(crate) handled_sessions: BTreeSet<String>,
}

pub(crate) fn sweep(
    launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
    force_active: bool,
) -> Result<SweepResult> {
    let store = ArmedRotateStore::in_state_dir(&context.paths.state);
    let records = store.records();
    if records.is_empty() {
        return Ok(SweepResult::default());
    }

    let scope = worker_scope(launcher, args)?;
    let now = timestamp(context.now);
    let inventory = inventory(context, &scope).unwrap_or_default();
    let live = OrchestratorStore::new(&context.paths.orchestrators)
        .live(&SystemProcessProbe, context.now)
        .unwrap_or_default();
    let claims =
        RotationClaimStore::new(&context.paths.rotation_claims, &context.paths.rotation_lock);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);

    let mut result = SweepResult::default();
    for (profile, marker) in records {
        if !fresh(&marker, context.now) {
            continue;
        }
        if is_rooted_but_not_absolute(&profile) {
            continue;
        }
        let profile = normalize_profile_path(profile);
        let live_record = live.iter().find(|record| {
            profile_for_record(record, &context.paths.home).as_deref() == Some(profile.as_path())
        });
        if live_record
            .is_some_and(|record| !force_active && recently_active(record, &profile, context.now))
            || (live_record.is_none() && !force_active && profile_recently_active(&profile))
        {
            continue;
        }

        let source = inventory
            .iter()
            .find(|account| same_profile(&account.profile, &profile))
            .cloned()
            .or_else(|| fallback_source(&profile, live_record, context, &usage));
        let Some(source) = source else {
            continue;
        };
        if !over_threshold(&source, &marker, now) {
            continue;
        }
        if !claims.try_claim(&profile, now)? {
            continue;
        }
        if let Some(record) = live_record {
            if marker
                .session
                .as_deref()
                .is_some_and(|owner| owner != record.session)
            {
                let _ = claims.release(&profile);
                continue;
            }
            let claimed = store.claim(&profile, Some(&record.session), context.now)?;
            if claimed.is_none() {
                let _ = claims.release(&profile);
                continue;
            }
            let mut handoff_args = vec![
                "--engine".into(),
                record.engine.to_string(),
                "--session".into(),
                record.session.clone(),
                "--reason".into(),
                "armed rotation tick".into(),
            ];
            for selector in &marker.prefer {
                if !is_rooted_but_not_absolute(Path::new(selector)) {
                    handoff_args.extend(["--to".into(), selector.clone()]);
                }
            }
            if parser::has(args, "--json") {
                handoff_args.push("--json".into());
            }
            handoff::run_live_with_trigger(
                launcher,
                context,
                &handoff_args,
                record,
                neomax_core::orchestration::continuation::RotationTrigger::Tick,
                false,
            )?;
            result.handled_sessions.insert(record.session.clone());
            result.reports.push(ArmedRotationReport {
                profile,
                engine: record.engine.to_string(),
                session: Some(record.session.clone()),
                status: "handoff started".into(),
            });
            continue;
        }

        let report = rotate_untracked(&profile, &source, &marker, &inventory, &controls, context);
        match report {
            Ok(Some(report)) => result.reports.push(report),
            Ok(None) => {
                let _ = claims.release(&profile);
            }
            Err(error) => {
                let _ = claims.release(&profile);
                result.reports.push(ArmedRotationReport {
                    profile,
                    engine: source.engine.to_string(),
                    session: None,
                    status: format!("rotation failed: {error}"),
                });
            }
        }
    }
    Ok(result)
}

fn inventory(context: &RuntimeContext, scope: &WorkerScope) -> Result<Vec<AccountSnapshot>> {
    let runtime = context.provider_runtime()?;
    let runs = RunStore::new(&context.paths.runs);
    let probe = SystemProcessProbe;
    let live_work = RunLiveWorkSource::with_system(&runs, &probe);
    let usage = UsageCacheStore::new(&context.paths.usage);
    let controls = AccountControlStore::new(&context.paths.cooldowns, &context.paths.paused);
    let inventory = AccountInventory {
        providers: runtime.registry(),
        quota: &usage,
        controls: &controls,
        live_work: &live_work,
    };
    Ok(inventory.routing_snapshots(scope, timestamp(context.now))?)
}

fn worker_scope(launcher: Launcher, args: &[String]) -> Result<WorkerScope> {
    let workers = parser::value(args, "--workers")?
        .map(|value| value.parse())
        .transpose()?;
    let engine = parser::value(args, "--engine")?
        .map(|value| value.parse())
        .transpose()?
        .map(WorkerScope::only);
    let explicit = workers.or(engine);
    launch::effective_worker_scope(launcher, explicit)
}

fn rotate_untracked(
    profile: &Path,
    source: &AccountSnapshot,
    marker: &ArmedRotateRecord,
    accounts: &[AccountSnapshot],
    controls: &AccountControlStore,
    context: &RuntimeContext,
) -> Result<Option<ArmedRotationReport>> {
    if is_rooted_but_not_absolute(profile) {
        anyhow::bail!(
            "armed rotation profile must not be rooted without an absolute prefix: {}",
            profile.display()
        );
    }
    if is_rooted_but_not_absolute(&source.profile) {
        anyhow::bail!(
            "armed rotation source profile must not be rooted without an absolute prefix: {}",
            source.profile.display()
        );
    }
    if !source.authenticated
        || source.paused
        || source
            .cooldown_until
            .is_some_and(|until| until > timestamp(context.now))
    {
        return Ok(None);
    }
    if !matches!(source.engine, Engine::Claude | Engine::Codex) {
        let mut handoff_args = vec![
            "--engine".into(),
            source.engine.to_string(),
            "--from".into(),
            source.account.clone(),
            "--base".into(),
            context.cwd.to_string_lossy().into_owned(),
            "--reason".into(),
            "armed rotation tick".into(),
        ];
        if let Some(session) = marker.session.as_deref() {
            handoff_args.extend(["--session".into(), session.into()]);
        }
        for selector in &marker.prefer {
            if !is_rooted_but_not_absolute(Path::new(selector)) {
                handoff_args.extend(["--to".into(), selector.clone()]);
            }
        }
        handoff::run_untracked_with_trigger(
            Launcher::Universal,
            context,
            &handoff_args,
            profile,
            neomax_core::orchestration::continuation::RotationTrigger::Tick,
        )?;
        return Ok(Some(ArmedRotationReport {
            profile: profile.to_path_buf(),
            engine: source.engine.to_string(),
            session: marker.session.clone(),
            status: "same-provider handoff started".into(),
        }));
    }
    if !source.rotation_eligible {
        return Ok(None);
    }
    let target = select_target(&HandoffTargetRequest {
        accounts,
        engine: source.engine,
        current_profile: profile,
        selectors: &marker.prefer,
        now: timestamp(context.now),
        policy: &TargetPolicy {
            allow_reserved: true,
            ..TargetPolicy::default()
        },
    })?;
    if is_rooted_but_not_absolute(&target.account.profile) {
        anyhow::bail!(
            "armed rotation target profile must not be rooted without an absolute prefix: {}",
            target.account.profile.display()
        );
    }
    if !target.account.rotation_eligible {
        return Ok(None);
    }
    let rotation_paths = neomax_core::orchestration::auth::RotationPaths::new(
        context.paths.state.join("auth-backups"),
        context.paths.state.join("auth-rotations.jsonl"),
    )
    .with_usage_cache_dir(context.paths.usage.clone());
    let rotation = neomax_core::orchestration::auth::RotationService::filesystem(rotation_paths);
    rotation.swap(
        source.engine,
        profile,
        &target.account.profile,
        context.now,
        Some("armed rotation tick".into()),
    )?;
    let reset = source
        .five_hour_reset_at
        .or(source.weekly_reset_at)
        .map(|value| value.timestamp() as f64);
    controls.set_cooldown(
        &target.account.profile,
        reset,
        context.now as f64,
        30.0 * 60.0,
    )?;
    Ok(Some(ArmedRotationReport {
        profile: profile.to_path_buf(),
        engine: source.engine.to_string(),
        session: None,
        status: format!("rotated to {}", target.account.account),
    }))
}

fn fallback_source(
    profile: &Path,
    record: Option<&OrchestratorRecord>,
    context: &RuntimeContext,
    usage: &UsageCacheStore,
) -> Option<AccountSnapshot> {
    if is_rooted_but_not_absolute(profile) {
        return None;
    }
    let engine = record
        .map(|record| record.engine)
        .unwrap_or_else(|| infer_engine(profile));
    let mut source = AccountSnapshot {
        engine,
        account: record
            .and_then(|record| record.account.map(|account| account.to_string()))
            .or_else(|| {
                profile
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "solo".into()),
        profile: profile.to_path_buf(),
        binary_available: true,
        authenticated: true,
        rotation_eligible: matches!(engine, Engine::Claude | Engine::Codex),
        paused: false,
        reserved: false,
        live_workers: 0,
        five_hour_percent: None,
        weekly_percent: None,
        cooldown_until: None,
        five_hour_reset_at: None,
        weekly_reset_at: None,
    };
    usage.hydrate(&mut source, timestamp(context.now));
    Some(source)
}

fn infer_engine(profile: &Path) -> Engine {
    let text = profile.to_string_lossy().to_ascii_lowercase();
    Engine::ALL
        .into_iter()
        .find(|engine| {
            let spec = neomax_core::providers::catalog::spec(*engine);
            text.contains(&spec.default_profile_dir.to_ascii_lowercase())
        })
        .unwrap_or(Engine::Claude)
}

fn over_threshold(
    source: &AccountSnapshot,
    marker: &ArmedRotateRecord,
    now: DateTime<Utc>,
) -> bool {
    (engine_has_five_hour(source.engine) && source.five_hour_at(now) >= marker.threshold)
        || source.weekly_at(now) >= marker.weekly_threshold
}

fn fresh(marker: &ArmedRotateRecord, now: i64) -> bool {
    now.saturating_sub(marker.ts) <= neomax_core::orchestration::rotation::ARMED_ROTATE_AGE_SECONDS
}

fn profile_for_record(record: &OrchestratorRecord, home: &Path) -> Option<PathBuf> {
    let value = Path::new(&record.account_dir);
    if is_rooted_but_not_absolute(value) || is_rooted_but_not_absolute(home) {
        return None;
    }
    if value.as_os_str().is_empty() {
        return Some(normalize_profile_path(home.join(
            neomax_core::providers::catalog::spec(record.engine).default_profile_dir,
        )));
    }
    let path = if value.is_absolute() {
        value.to_path_buf()
    } else {
        home.join(value)
    };
    if is_rooted_but_not_absolute(&path) {
        return None;
    }
    Some(normalize_profile_path(path))
}

fn same_profile(left: &Path, right: &Path) -> bool {
    !is_rooted_but_not_absolute(left)
        && !is_rooted_but_not_absolute(right)
        && normalize_profile_path(left) == normalize_profile_path(right)
}

fn recently_active(record: &OrchestratorRecord, profile: &Path, now: i64) -> bool {
    now.saturating_sub(record.last_seen) <= ACTIVE_GRACE_SECONDS || profile_recently_active(profile)
}

fn profile_recently_active(profile: &Path) -> bool {
    if is_rooted_but_not_absolute(profile) {
        return false;
    }
    let projects = profile.join("projects");
    recent_jsonl(&projects, 0)
}

fn recent_jsonl(path: &Path, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            return recent_jsonl(&path, depth + 1);
        }
        path.extension().and_then(|value| value.to_str()) == Some("jsonl")
            && entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age.as_secs() <= ACTIVE_GRACE_SECONDS as u64)
    })
}

fn timestamp(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_markers_follow_the_reference_age_boundary() {
        let marker = ArmedRotateRecord {
            ts: 100,
            ..ArmedRotateRecord::default()
        };
        assert!(fresh(
            &marker,
            100 + neomax_core::orchestration::rotation::ARMED_ROTATE_AGE_SECONDS
        ));
        assert!(!fresh(
            &marker,
            101 + neomax_core::orchestration::rotation::ARMED_ROTATE_AGE_SECONDS
        ));
    }

    #[test]
    fn threshold_uses_five_hour_or_weekly_window() {
        let temp = tempfile::tempdir().expect("temporary root");
        let source = AccountSnapshot {
            engine: Engine::Claude,
            account: "one".into(),
            profile: temp.path().join("profiles/one"),
            binary_available: true,
            authenticated: true,
            rotation_eligible: true,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: Some(98.0),
            weekly_percent: Some(10.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        };
        let marker = ArmedRotateRecord {
            threshold: 98.0,
            weekly_threshold: 99.0,
            ..ArmedRotateRecord::default()
        };
        assert!(over_threshold(&source, &marker, timestamp(100)));
    }

    #[test]
    fn profile_paths_are_compared_after_normalization() {
        let temp = tempfile::tempdir().expect("temporary root");
        let profile = temp.path().join("profiles/one");
        let traversing = temp.path().join("profiles/one/../one");
        assert!(same_profile(&traversing, &profile,));
    }

    #[cfg(windows)]
    #[test]
    fn ignores_windows_partial_root_orchestrator_account_directories() {
        let mut record = OrchestratorRecord {
            session: "session".into(),
            pid: None,
            engine: Engine::Claude,
            account: Some(1),
            account_dir: r"\rooted".into(),
            project: None,
            branch_prefix: None,
            cwd: PathBuf::from(r"C:\workspace"),
            model: "claude-fable-5[1m]".into(),
            reserved: false,
            started: 0,
            last_seen: 0,
            live: false,
            process_state: neomax_core::runs::ProbeState::Unknown,
            extra: Default::default(),
        };
        assert_eq!(
            profile_for_record(&record, Path::new(r"C:\Users\fixture")),
            None
        );

        record.account_dir = r"C:drive-relative".into();
        assert_eq!(
            profile_for_record(&record, Path::new(r"C:\Users\fixture")),
            None
        );
    }
}
