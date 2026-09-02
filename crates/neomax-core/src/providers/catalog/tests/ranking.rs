use std::path::PathBuf;

use super::super::{
    orchestrator_eligibility, rank_neomax, spec, AuthMethod, AuthStatus, BinaryStatus, Eligibility,
    OrchestratorCandidate, ProfileEligibility, ProfileSnapshot, ProviderSnapshot, RankingPolicy,
    DEFAULT_NEOMAX_PRIORITY,
};
use crate::Engine;

#[test]
fn neomax_ranking_rejects_hard_wall_and_prefers_headroom() {
    let temp = tempfile::tempdir().unwrap();
    let profiles = [
        ProfileSnapshot {
            engine: Engine::Claude,
            account: "1".into(),
            path: temp.path().join(".claude"),
            reserved: false,
            auth: AuthStatus::Authenticated {
                methods: vec![AuthMethod::OAuth],
            },
            eligibility: ProfileEligibility {
                credential_present: true,
                authenticated: true,
                worker_eligible: true,
                orchestrator_eligible: true,
                rotation_eligible: true,
                managed_pool_eligible: true,
            },
        },
        ProfileSnapshot {
            engine: Engine::Opencode,
            account: "1".into(),
            path: temp.path().join(".opencode"),
            reserved: false,
            auth: AuthStatus::Authenticated {
                methods: vec![AuthMethod::ApiKey],
            },
            eligibility: ProfileEligibility {
                credential_present: true,
                authenticated: true,
                worker_eligible: true,
                orchestrator_eligible: true,
                rotation_eligible: true,
                managed_pool_eligible: true,
            },
        },
    ];
    let candidates = [
        OrchestratorCandidate::new(&profiles[0], Some(99.0), 0, false),
        OrchestratorCandidate::new(&profiles[1], Some(40.0), 2, false),
    ];
    let ranked = rank_neomax(
        candidates,
        &DEFAULT_NEOMAX_PRIORITY,
        RankingPolicy::default(),
    );
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].engine, Engine::Opencode);
    assert_eq!(
        orchestrator_eligibility(&ProviderSnapshot {
            spec: spec(Engine::Opencode),
            binary: BinaryStatus {
                program: "opencode".into(),
                available: true,
                version: None,
            },
            profiles: vec![profiles[1].clone()],
            models: Vec::new(),
        }),
        Eligibility::Eligible
    );
    assert_eq!(profiles[0].path, PathBuf::from(temp.path()).join(".claude"));
}
