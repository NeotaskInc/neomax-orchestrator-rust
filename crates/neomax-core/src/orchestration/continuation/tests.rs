use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};

use crate::Engine;
use crate::accounts::AccountSnapshot;
use crate::orchestration::auth::RotationEffects;
use crate::orchestration::handoff::HandoffBaton;
use crate::runs::RunRecord;
use crate::runs::failover::FailoverTarget;

use super::ports::{CredentialRotationPort, HandoffPort};
use super::request::{ContinuationRequest, RotationTrigger};
use super::service::{ContinuationMode, ContinuationOutcome, ContinuationService};
use super::state::apply_to_run;

#[derive(Clone)]
struct FakeRotation {
    supported: bool,
    fail: bool,
    calls: Arc<Mutex<Vec<(Engine, PathBuf, PathBuf)>>>,
}

impl CredentialRotationPort for FakeRotation {
    fn supports(&self, _engine: Engine) -> bool {
        self.supported
    }

    fn swap(
        &self,
        engine: Engine,
        destination: &std::path::Path,
        source: &std::path::Path,
        _timestamp: i64,
        _reason: Option<String>,
    ) -> crate::Result<RotationEffects> {
        self.calls
            .lock()
            .unwrap()
            .push((engine, destination.to_path_buf(), source.to_path_buf()));
        if self.fail {
            return Err(crate::Error::Message("fixture rotation failed".into()));
        }
        Ok(RotationEffects {
            engine,
            operation: crate::orchestration::auth::RotationOperation::Swap,
            destination: destination.to_path_buf(),
            source: Some(source.to_path_buf()),
            backup_paths: Vec::new(),
            invalidated_cache_paths: Vec::new(),
        })
    }
}

#[derive(Clone, Default)]
struct FakeHandoff {
    batons: Arc<Mutex<Vec<HandoffBaton>>>,
    fail: bool,
}

impl HandoffPort for FakeHandoff {
    fn save(&self, baton: &HandoffBaton) -> crate::Result<()> {
        if self.fail {
            return Err(crate::Error::Message("fixture handoff failed".into()));
        }
        self.batons.lock().unwrap().push(baton.clone());
        Ok(())
    }
}

fn request(engine: Engine, target_engine: Engine, trigger: RotationTrigger) -> ContinuationRequest {
    let now = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    ContinuationRequest {
        run_id: "run-1".into(),
        engine,
        source_profile: PathBuf::from(format!("/profiles/{engine}-1")),
        source_account: "1".into(),
        source_rotation_eligible: true,
        target: AccountSnapshot {
            engine: target_engine,
            account: "2".into(),
            profile: PathBuf::from(format!("/profiles/{target_engine}-2")),
            binary_available: true,
            authenticated: true,
            rotation_eligible: true,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: Some(12.0),
            weekly_percent: Some(20.0),
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        },
        trigger,
        reason: "weekly 99%".into(),
        now,
        cwd: PathBuf::from("/workspace"),
        workdir: PathBuf::from("/workspace/worktree"),
        repo: Some(PathBuf::from("/workspace/repo")),
        worktree: Some(PathBuf::from("/workspace/worktree")),
        branch: Some("neomax/run-1".into()),
        base: Some("main".into()),
        project: Some("project".into()),
        session: Some("session-1".into()),
        prompt: "continue work".into(),
        attempt: 3,
        five_hour: Some(99.0),
        seven_day: Some(42.0),
        resets_at: Some(1_700_000_500.0),
        limit_window: Some("5h".into()),
        run_state: BTreeMap::new(),
    }
}

#[test]
fn quota_uses_in_place_auth_rotation_and_cools_the_spent_slot() {
    let rotation = FakeRotation {
        supported: true,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let mut request = request(Engine::Claude, Engine::Claude, RotationTrigger::Quota);
    request
        .run_state
        .insert("task_id".into(), serde_json::json!("task-7"));
    let outcome = service.continue_run(&request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::InPlaceAuthRotation);
    assert_eq!(outcome.target_profile, request.source_profile);
    assert_eq!(outcome.cooldown_profile, request.target.profile);
    assert_eq!(outcome.resume_session.as_deref(), Some("session-1"));
    assert_eq!(rotation.calls.lock().unwrap().len(), 1);
    let baton = &handoff.batons.lock().unwrap()[0];
    assert_eq!(baton.extra["run_id"], "run-1");
    assert_eq!(baton.extra["branch"], "neomax/run-1");
    assert_eq!(baton.extra["session"], "session-1");
    assert_eq!(baton.extra["run_state"]["task_id"], "task-7");
    assert_eq!(baton.five_hour, Some(99.0));
    assert_eq!(baton.seven_day, Some(42.0));
}

#[test]
fn isolated_profiles_use_same_provider_handoff_and_preserve_work_state() {
    let rotation = FakeRotation {
        supported: false,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let request = request(Engine::Opencode, Engine::Opencode, RotationTrigger::Quota);
    let outcome = service.continue_run(&request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::SameProviderHandoff);
    assert_eq!(outcome.target_profile, request.target.profile);
    assert_eq!(outcome.cooldown_profile, request.source_profile);
    assert!(outcome.resume_session.is_none());
    assert!(rotation.calls.lock().unwrap().is_empty());
    assert_eq!(handoff.batons.lock().unwrap().len(), 1);
}

#[test]
fn same_provider_handoff_clears_source_session_for_every_provider() {
    for engine in Engine::ALL {
        let target = request(engine, engine, RotationTrigger::Quota).target;
        let target_profile = target.profile.clone();
        let mut run = RunRecord::new(
            format!("run-{engine}"),
            engine,
            "model",
            "continue work",
            format!("/profiles/{engine}-1"),
            "/workspace",
            1,
        );
        run.session = Some("session-1".into());
        run.repo = Some(PathBuf::from("/workspace/repo"));
        run.worktree = Some(PathBuf::from("/workspace/worktree"));
        run.branch = Some("neomax/run-1".into());
        run.project = Some("project".into());

        apply_to_run(
            &mut run,
            &FailoverTarget {
                account: target,
                crosses_provider: false,
            },
            &ContinuationOutcome {
                mode: ContinuationMode::SameProviderHandoff,
                target_engine: engine,
                target_profile: target_profile.clone(),
                cooldown_profile: PathBuf::from(format!("/profiles/{engine}-1")),
                resume_session: None,
                rotation_effects: None,
            },
        );

        assert_eq!(run.profile, target_profile, "provider {engine}");
        assert_eq!(run.session, None, "provider {engine}");
        assert_eq!(run.session_history[0].session, "session-1");
        assert_eq!(run.repo, Some(PathBuf::from("/workspace/repo")));
        assert_eq!(run.worktree, Some(PathBuf::from("/workspace/worktree")));
        assert_eq!(run.branch.as_deref(), Some("neomax/run-1"));
        assert_eq!(run.project.as_deref(), Some("project"));
    }
}

#[test]
fn cross_provider_handoff_changes_routing_without_losing_work_metadata() {
    let source = Engine::Claude;
    let target_engine = Engine::Kimi;
    let target = request(source, target_engine, RotationTrigger::Tick).target;
    let target_profile = target.profile.clone();
    let mut run = RunRecord::new(
        "cross-provider-run",
        source,
        "claude-fable-5[1m]",
        "continue work",
        "/profiles/claude-1",
        "/workspace",
        1,
    );
    run.session = Some("session-1".into());
    run.repo = Some(PathBuf::from("/workspace/repo"));
    run.worktree = Some(PathBuf::from("/workspace/worktree"));
    run.branch = Some("neomax/run-1".into());
    run.project = Some("project".into());

    apply_to_run(
        &mut run,
        &FailoverTarget {
            account: target,
            crosses_provider: true,
        },
        &ContinuationOutcome {
            mode: ContinuationMode::CrossProviderHandoff,
            target_engine,
            target_profile: target_profile.clone(),
            cooldown_profile: PathBuf::from("/profiles/claude-1"),
            resume_session: None,
            rotation_effects: None,
        },
    );

    assert_eq!(run.engine, target_engine);
    assert_eq!(run.profile, target_profile);
    assert_eq!(run.session, None);
    assert_eq!(run.session_history[0].session, "session-1");
    assert_eq!(run.repo, Some(PathBuf::from("/workspace/repo")));
    assert_eq!(run.worktree, Some(PathBuf::from("/workspace/worktree")));
    assert_eq!(run.branch.as_deref(), Some("neomax/run-1"));
    assert_eq!(run.project.as_deref(), Some("project"));
}

#[test]
fn kimi_handoff_baton_keeps_prompt_session_and_tool_state() {
    let rotation = FakeRotation {
        supported: false,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let mut request = request(Engine::Kimi, Engine::Kimi, RotationTrigger::Quota);
    request
        .run_state
        .insert("tool_policy".into(), serde_json::json!("orchestrator"));
    let outcome = service.continue_run(&request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::SameProviderHandoff);
    let batons = handoff.batons.lock().unwrap();
    assert_eq!(batons.len(), 1);
    assert_eq!(batons[0].extra["prompt"], "continue work");
    assert_eq!(batons[0].extra["session"], "session-1");
    assert_eq!(batons[0].extra["run_state"]["tool_policy"], "orchestrator");
}

#[test]
fn api_key_target_uses_same_provider_handoff_without_credential_copy() {
    let rotation = FakeRotation {
        supported: true,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let mut request = request(Engine::Claude, Engine::Claude, RotationTrigger::Quota);
    request.target.rotation_eligible = false;
    let outcome = service.continue_run(&request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::SameProviderHandoff);
    assert_eq!(outcome.target_profile, request.target.profile);
    assert!(rotation.calls.lock().unwrap().is_empty());
    assert_eq!(handoff.batons.lock().unwrap().len(), 1);
}

#[test]
fn quota_continuation_stays_with_each_provider_before_any_cross_provider_fallback() {
    for engine in Engine::ALL {
        let rotation = FakeRotation {
            supported: matches!(engine, Engine::Claude | Engine::Codex),
            fail: false,
            calls: Arc::default(),
        };
        let handoff = FakeHandoff::default();
        let service = ContinuationService {
            rotation: &rotation,
            handoff: &handoff,
        };
        let request = request(engine, engine, RotationTrigger::Quota);
        let outcome = service.continue_run(&request).unwrap();
        let expected = if rotation.supported {
            ContinuationMode::InPlaceAuthRotation
        } else {
            ContinuationMode::SameProviderHandoff
        };
        assert_eq!(outcome.mode, expected, "provider {engine}");
        assert_eq!(outcome.target_engine, engine);
        assert_eq!(handoff.batons.lock().unwrap().len(), 1);
    }
}

#[test]
fn generic_manual_rotation_cannot_cross_provider() {
    let rotation = FakeRotation {
        supported: false,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let request = request(Engine::Claude, Engine::Opencode, RotationTrigger::Manual);
    let error = service.continue_run(&request).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("only after a quota event or maintenance tick")
    );
    assert!(handoff.batons.lock().unwrap().is_empty());
}

#[test]
fn quota_tick_can_cross_provider_after_same_provider_is_exhausted() {
    let rotation = FakeRotation {
        supported: false,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let request = request(Engine::Claude, Engine::Opencode, RotationTrigger::Tick);
    let outcome = service.tick(request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::CrossProviderHandoff);
    assert_eq!(outcome.target_engine, Engine::Opencode);
    assert_eq!(handoff.batons.lock().unwrap().len(), 1);
}

#[test]
fn unknown_quota_values_are_omitted_from_the_baton() {
    let rotation = FakeRotation {
        supported: false,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let mut request = request(Engine::Opencode, Engine::Opencode, RotationTrigger::Quota);
    request.seven_day = None;
    let outcome = service.continue_run(&request).unwrap();
    assert_eq!(outcome.mode, ContinuationMode::SameProviderHandoff);
    let batons = handoff.batons.lock().unwrap();
    let baton = &batons[0];
    assert!(baton.five_hour.is_none());
    assert!(baton.seven_day.is_none());
    let json = serde_json::to_value(baton).unwrap();
    assert!(json.get("five_hour").is_none());
    assert!(json.get("seven_day").is_none());
}

#[test]
fn failed_credential_rotation_does_not_write_a_handoff_baton() {
    let rotation = FakeRotation {
        supported: true,
        fail: true,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff::default();
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let request = request(Engine::Claude, Engine::Claude, RotationTrigger::Quota);
    assert!(service.continue_run(&request).is_err());
    assert!(handoff.batons.lock().unwrap().is_empty());
}

#[test]
fn failed_handoff_state_rolls_back_a_successful_credential_swap() {
    let rotation = FakeRotation {
        supported: true,
        fail: false,
        calls: Arc::default(),
    };
    let handoff = FakeHandoff {
        batons: Arc::default(),
        fail: true,
    };
    let service = ContinuationService {
        rotation: &rotation,
        handoff: &handoff,
    };
    let request = request(Engine::Claude, Engine::Claude, RotationTrigger::Quota);
    let error = service.continue_run(&request).unwrap_err();
    assert!(error.to_string().contains("credentials rolled back"));
    assert_eq!(rotation.calls.lock().unwrap().len(), 2);
}
