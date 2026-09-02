use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::Utc;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::agent_tools::LaunchRole;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::registry::{OrchestratorRegistration, OrchestratorStore};
use neomax_core::providers::orchestrator::ORCHESTRATOR_ORIENTATION_ENV;
use neomax_core::runs::{RunRecord, RunStatus, RunStore, run_id};
use neomax_core::{Engine, WorkerScope};
use serde_json::json;

use crate::context::RuntimeContext;

pub(super) struct NewRunInput<'a> {
    pub(super) launcher: Launcher,
    pub(super) id: &'a str,
    pub(super) engine: Engine,
    pub(super) model: &'a str,
    pub(super) prompt: &'a str,
    pub(super) profile: PathBuf,
    pub(super) workdir: PathBuf,
    pub(super) goal: Option<String>,
    pub(super) base: Option<String>,
    pub(super) tag: Option<String>,
    pub(super) max_turns: Option<u32>,
    pub(super) session: Option<String>,
    pub(super) effort: Option<String>,
    pub(super) wall_min: Option<f64>,
    pub(super) stall_min: Option<f64>,
    pub(super) no_failover: bool,
    pub(super) no_worktree: bool,
    pub(super) plan_mode: bool,
    pub(super) open_pull_request: bool,
    pub(super) ultra: bool,
    pub(super) opus: bool,
    pub(super) brief: bool,
    pub(super) solo: bool,
    pub(super) context: &'a RuntimeContext,
    pub(super) scope: &'a WorkerScope,
    pub(super) launch_role: LaunchRole,
    pub(super) orchestrator_reserved: bool,
    pub(super) worker_models: BTreeMap<Engine, String>,
}

pub(super) fn new_run(input: NewRunInput<'_>) -> RunRecord {
    let NewRunInput {
        launcher,
        id,
        engine,
        model,
        prompt,
        profile,
        workdir,
        goal,
        base,
        tag,
        max_turns,
        session,
        effort,
        wall_min,
        stall_min,
        no_failover,
        no_worktree,
        plan_mode,
        open_pull_request,
        ultra,
        opus,
        brief,
        solo,
        context,
        scope,
        launch_role,
        orchestrator_reserved,
        worker_models,
    } = input;
    let mut run: RunRecord = serde_json::from_value(json!({
        "id": id,
        "engine": engine,
        "model": model,
        "prompt": prompt,
        "profile": profile,
        "workdir": workdir,
        "cwd": context.cwd,
        "base": base,
        "base_ref": base,
        "tag": tag,
        "goal": goal,
        "max_turns": max_turns,
        "session": session,
        "effort": effort,
        "wall_min": wall_min,
        "stall_min": stall_min,
        "no_failover": no_failover,
        "plan_mode": plan_mode,
        "pr": open_pull_request,
        "ultra": ultra,
        "opus": opus,
        "project": context.project_for_cwd(),
        "status": "running",
        "started": context.now,
        "attempt": 1
    }))
    .expect("launch run record is valid JSON");
    run.extra.insert("worker_scope".into(), scope.csv().into());
    run.extra
        .insert("orchestrator".into(), engine.to_string().into());
    run.extra.insert("no_worktree".into(), no_worktree.into());
    run.extra.insert("brief".into(), brief.into());
    run.extra.insert("solo".into(), solo.into());
    run.extra
        .insert("orchestrator_reserved".into(), orchestrator_reserved.into());
    if let Ok(models) = serde_json::to_value(
        worker_models
            .iter()
            .map(|(engine, model)| (engine.to_string(), model))
            .collect::<BTreeMap<_, _>>(),
    ) {
        run.extra.insert("worker_models".into(), models);
    }
    run.environment
        .insert("NEOMAX_ROLE".into(), engine.to_string());
    run.environment
        .insert("NEOMAX_ENGINE".into(), engine.to_string());
    run.environment.insert("NEOMAX_FLEET".into(), scope.csv());
    if launch_role.is_orchestrator() && prompt.trim().is_empty() {
        if let Ok(orientation) =
            crate::operations::no_task_orientation(launcher, engine, scope, &worker_models, context)
        {
            run.environment
                .insert(ORCHESTRATOR_ORIENTATION_ENV.into(), orientation);
        }
    }
    run.launch_role = launch_role;
    run
}

pub(super) fn register_orchestrator(
    store: &OrchestratorStore,
    run: &RunRecord,
    account: &AccountSnapshot,
    context: &RuntimeContext,
    model: &str,
    session: &Option<String>,
    reserved: bool,
) -> neomax_core::Result<()> {
    let session = session.as_deref().unwrap_or(&run.id);
    let mut metadata = BTreeMap::new();
    if let Some(scope) = run.extra.get("worker_scope") {
        metadata.insert("worker_scope".into(), scope.clone());
    }
    metadata.insert("run_id".into(), json!(run.id.clone()));
    store.register_with_metadata(
        OrchestratorRegistration {
            session: session.into(),
            pid: run.supervisor_pid,
            engine: run.engine,
            account: orchestrator_account_number(account),
            account_dir: account
                .profile
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .into(),
            project: run.project.clone().or_else(|| context.project_for_cwd()),
            branch_prefix: run.branch.clone(),
            cwd: run.workdir.clone(),
            model: model.into(),
            reserved,
            now: context.now,
        },
        metadata,
    )?;
    Ok(())
}

fn orchestrator_account_number(account: &AccountSnapshot) -> Option<u32> {
    neomax_core::providers::catalog::profile_account_number(account.engine, &account.profile)
        .or_else(|| account.account.parse().ok())
}

pub(super) fn finish_failed(store: &RunStore, id: &str, now: i64, error: &str) {
    let _ = store.update(id, |run| {
        if run.killed || run.status.is_interruption() {
            if run.killed && !run.status.is_interruption() {
                run.status = RunStatus::Aborted;
                run.ended.get_or_insert(now);
            }
            return Ok(());
        }
        run.status = RunStatus::Error;
        run.ended = Some(now);
        run.error_detail = Some(error.into());
        run.worker_pid = None;
        Ok(())
    });
}

pub(super) fn next_run_id(store: &RunStore, now: i64) -> String {
    let base = run_id(Utc::now(), std::process::id());
    if !store.path(&base).exists() {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !store.path(&candidate).exists() {
            return candidate;
        }
        suffix += 1;
        if suffix > 10_000 {
            return format!("{base}-{}", now.max(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::{
        ORCHESTRATOR_ORIENTATION_ENV, finish_failed, next_run_id, orchestrator_account_number,
        register_orchestrator,
    };
    use neomax_core::Engine;
    use neomax_core::accounts::AccountSnapshot;
    use neomax_core::orchestration::registry::OrchestratorStore;
    use neomax_core::runs::{ProcessProbe, RunRecord, RunStore};

    use crate::tests::fixture;

    fn account(engine: Engine, label: &str, profile: &str) -> AccountSnapshot {
        AccountSnapshot {
            engine,
            account: label.into(),
            profile: PathBuf::from(profile),
            binary_available: true,
            authenticated: true,
            rotation_eligible: false,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: None,
            weekly_percent: None,
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        }
    }

    struct LiveProbe;

    impl ProcessProbe for LiveProbe {
        fn pid_alive(&self, _pid: u32) -> bool {
            true
        }

        fn worker_alive(&self, _worker_pid: u32, _engine: Engine) -> bool {
            true
        }
    }

    #[test]
    fn run_ids_avoid_clobbering_a_same_second_launch() {
        let temp = tempfile::tempdir().unwrap();
        let store = RunStore::new(temp.path());
        let first = next_run_id(&store, 1);
        let record: neomax_core::runs::RunRecord = serde_json::from_value(serde_json::json!({
            "id": first,
            "status": "running",
            "started": 1
        }))
        .unwrap();
        store.create(&record).unwrap();
        assert_ne!(next_run_id(&store, 1), record.id);
    }

    #[test]
    fn registry_account_numbers_follow_provider_profile_names() {
        let cases = [
            (Engine::Claude, "1", "/profiles/.claude"),
            (Engine::Claude, "2", "/profiles/.claude-acct2"),
            (Engine::Codex, "2", "/profiles/.codex-acct2"),
            (Engine::Opencode, "3", "/profiles/.opencode-acct3"),
            (Engine::Kimi, "4", "/profiles/.kimi-code-acct4"),
            (Engine::Grok, "5", "/profiles/.grok-acct5"),
        ];
        for (engine, label, profile) in cases {
            assert_eq!(
                orchestrator_account_number(&account(engine, label, profile)),
                label.parse().ok()
            );
        }
    }

    #[test]
    fn root_registry_round_trip_keeps_typed_identity_out_of_extra_metadata() {
        let fixture = fixture();
        let profile = fixture.context.paths.home.join(".claude-acct2");
        let mut run = RunRecord::new(
            "root-session",
            Engine::Claude,
            "fixture-model",
            "fixture task",
            profile,
            fixture.context.cwd.clone(),
            fixture.context.now,
        );
        run.project = Some("fixture-project".into());
        run.branch = Some("fixture/branch".into());
        run.supervisor_pid = Some(std::process::id());
        run.launch_role = neomax_core::agent_tools::LaunchRole::Orchestrator;
        run.extra
            .insert("worker_scope".into(), "claude,kimi".into());
        let account = account(Engine::Claude, "2", "/profiles/.claude-acct2");
        let store = OrchestratorStore::new(&fixture.context.paths.orchestrators);

        register_orchestrator(
            &store,
            &run,
            &account,
            &fixture.context,
            "fixture-model",
            &Some("root-session".into()),
            false,
        )
        .unwrap();

        let records = store.live(&LiveProbe, fixture.context.now).unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.session, "root-session");
        assert_eq!(record.account, Some(2));
        assert_eq!(record.project.as_deref(), Some("fixture-project"));
        assert_eq!(record.branch_prefix.as_deref(), Some("fixture/branch"));
        assert_eq!(record.extra.get("run_id"), Some(&"root-session".into()));
        assert_eq!(
            record.extra.get("worker_scope"),
            Some(&"claude,kimi".into())
        );
        assert!(!record.extra.contains_key("session"));
        assert!(!record.extra.contains_key("project"));
        assert!(!record.extra.contains_key("branch_prefix"));
    }

    #[test]
    fn explicit_root_tasks_do_not_receive_the_no_task_orientation() {
        let fixture = fixture();
        let scope = neomax_core::WorkerScope::all();
        let worker_models = BTreeMap::new();
        let run = super::new_run(super::NewRunInput {
            launcher: neomax_core::orchestration::commands::Launcher::ProviderOrchestrator(
                Engine::Codex,
            ),
            id: "explicit-task",
            engine: Engine::Codex,
            model: "fixture-model",
            prompt: "inspect the project",
            profile: fixture.context.paths.home.join(".codex"),
            workdir: fixture.context.cwd.clone(),
            goal: None,
            base: None,
            tag: Some("project=fixture".into()),
            max_turns: None,
            session: None,
            effort: None,
            wall_min: None,
            stall_min: None,
            no_failover: false,
            no_worktree: true,
            plan_mode: false,
            open_pull_request: false,
            ultra: false,
            opus: false,
            brief: false,
            solo: false,
            context: &fixture.context,
            scope: &scope,
            launch_role: neomax_core::agent_tools::LaunchRole::Orchestrator,
            orchestrator_reserved: false,
            worker_models,
        });
        assert!(!run.environment.contains_key(ORCHESTRATOR_ORIENTATION_ENV));
        assert_eq!(run.tag.as_deref(), Some("project=fixture"));
    }

    #[test]
    fn finish_failed_preserves_a_persisted_interruption() {
        let fixture = fixture();
        let store = RunStore::new(&fixture.context.paths.runs);
        let mut run = RunRecord::new(
            "interrupted",
            Engine::Codex,
            "fixture-model",
            "fixture task",
            fixture.context.paths.home.join(".codex"),
            fixture.context.cwd.clone(),
            fixture.context.now,
        );
        run.status = neomax_core::runs::RunStatus::Aborted;
        run.killed = true;
        run.interrupt_signal = Some(15);
        run.ended = Some(fixture.context.now - 1);
        store.create(&run).unwrap();

        finish_failed(&store, &run.id, fixture.context.now, "launch failed");

        let saved = store.load(&run.id).unwrap();
        assert_eq!(saved.status, neomax_core::runs::RunStatus::Aborted);
        assert!(saved.killed);
        assert_eq!(saved.interrupt_signal, Some(15));
        assert_eq!(saved.ended, Some(fixture.context.now - 1));
        assert!(saved.error_detail.is_none());
    }

    #[test]
    fn finish_failed_converts_a_kill_marker_without_overwriting_it_with_error() {
        let fixture = fixture();
        let store = RunStore::new(&fixture.context.paths.runs);
        let mut run = RunRecord::new(
            "killed",
            Engine::Codex,
            "fixture-model",
            "fixture task",
            fixture.context.paths.home.join(".codex"),
            fixture.context.cwd.clone(),
            fixture.context.now,
        );
        run.killed = true;
        store.create(&run).unwrap();

        finish_failed(&store, &run.id, fixture.context.now, "launch failed");

        let saved = store.load(&run.id).unwrap();
        assert_eq!(saved.status, neomax_core::runs::RunStatus::Aborted);
        assert!(saved.killed);
        assert_eq!(saved.ended, Some(fixture.context.now));
        assert!(saved.error_detail.is_none());
    }
}
