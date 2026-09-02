use std::path::{Path, PathBuf};

use crate::orchestration::auth::{
    FsCredentialWriter, RotationEffects, RotationPaths, RotationService,
};
use crate::config::StatePaths;
use crate::orchestration::handoff::HandoffStore;
use crate::Error;
use crate::Result;

use super::ports::{CredentialRotationPort, HandoffPort};
use super::request::{ContinuationRequest, RotationTrigger};
use super::state::build_baton;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationMode {
    InPlaceAuthRotation,
    SameProviderHandoff,
    CrossProviderHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationOutcome {
    pub mode: ContinuationMode,
    pub target_engine: crate::Engine,
    pub target_profile: std::path::PathBuf,
    pub cooldown_profile: std::path::PathBuf,
    pub resume_session: Option<String>,
    pub rotation_effects: Option<RotationEffects>,
}

pub struct ContinuationService<'a> {
    pub rotation: &'a dyn CredentialRotationPort,
    pub handoff: &'a dyn HandoffPort,
}

pub struct FilesystemContinuation {
    rotation: RotationService<FsCredentialWriter>,
    handoff: HandoffStore,
}

impl FilesystemContinuation {
    pub fn in_state_dir(state: impl AsRef<Path>, usage_cache: Option<std::path::PathBuf>) -> Self {
        Self::in_paths(
            &StatePaths::new(PathBuf::new(), state.as_ref().to_path_buf()),
            usage_cache,
        )
    }

    pub fn in_paths(paths: &StatePaths, usage_cache: Option<std::path::PathBuf>) -> Self {
        let mut rotation_paths = RotationPaths::new(
            paths.auth_backups.clone(),
            paths.auth_rotations.clone(),
        );
        if let Some(usage_cache) = usage_cache {
            rotation_paths = rotation_paths.with_usage_cache_dir(usage_cache);
        }
        Self {
            rotation: RotationService::filesystem(rotation_paths),
            handoff: HandoffStore::at_state_dir(&paths.state),
        }
    }
}

pub trait ContinuationPort: Send + Sync {
    fn continue_after_limit(&self, request: &ContinuationRequest) -> Result<ContinuationOutcome>;
}

impl ContinuationService<'_> {
    pub fn continue_run(&self, request: &ContinuationRequest) -> Result<ContinuationOutcome> {
        if request.source_profile == request.target.profile {
            return Err(Error::Conflict(
                "continuation target is the current account profile".into(),
            ));
        }
        if !request.target.authenticated {
            return Err(Error::Conflict(
                "continuation target is not authenticated".into(),
            ));
        }
        if request.target.paused {
            return Err(Error::Conflict("continuation target is paused".into()));
        }
        if request
            .target
            .cooldown_until
            .is_some_and(|until| until > request.now)
        {
            return Err(Error::Conflict(
                "continuation target is cooling down".into(),
            ));
        }
        if request.target.at_hard_wall(request.now) {
            return Err(Error::Conflict(
                "continuation target is at the usage wall".into(),
            ));
        }
        let same_provider = request.engine == request.target.engine;
        let baton = build_baton(request);
        if same_provider
            && request.source_rotation_eligible
            && request.target.rotation_eligible
            && self.rotation.supports(request.engine)
        {
            let effects = self.rotation.swap(
                request.engine,
                &request.source_profile,
                &request.target.profile,
                request.now.timestamp(),
                Some(request.reason.clone()),
            )?;
            if let Err(error) = self.handoff.save(&baton) {
                let rollback = self.rotation.rollback(
                    &effects,
                    request.now.timestamp(),
                    Some("rollback after handoff state failure".into()),
                );
                return Err(match rollback {
                    Ok(()) => Error::Message(format!(
                        "handoff state failed after credential rotation: {error}; credentials rolled back"
                    )),
                    Err(rollback_error) => Error::Message(format!(
                        "handoff state failed after credential rotation: {error}; credential rollback failed: {rollback_error}"
                    )),
                });
            }
            return Ok(ContinuationOutcome {
                mode: ContinuationMode::InPlaceAuthRotation,
                target_engine: request.engine,
                target_profile: request.source_profile.clone(),
                cooldown_profile: request.target.profile.clone(),
                resume_session: request.session.clone(),
                rotation_effects: Some(effects),
            });
        }
        if !same_provider && !request.trigger.allows_cross_provider() {
            return Err(Error::Conflict(
                "cross-provider continuation is permitted only after a quota event or maintenance tick".into(),
            ));
        }
        self.handoff.save(&baton)?;
        Ok(ContinuationOutcome {
            mode: if same_provider {
                ContinuationMode::SameProviderHandoff
            } else {
                ContinuationMode::CrossProviderHandoff
            },
            target_engine: request.target.engine,
            target_profile: request.target.profile.clone(),
            cooldown_profile: request.source_profile.clone(),
            resume_session: None,
            rotation_effects: None,
        })
    }

    pub fn manual(&self, mut request: ContinuationRequest) -> Result<ContinuationOutcome> {
        request.trigger = RotationTrigger::Manual;
        self.continue_run(&request)
    }

    pub fn tick(&self, mut request: ContinuationRequest) -> Result<ContinuationOutcome> {
        request.trigger = RotationTrigger::Tick;
        self.continue_run(&request)
    }
}

impl ContinuationPort for ContinuationService<'_> {
    fn continue_after_limit(&self, request: &ContinuationRequest) -> Result<ContinuationOutcome> {
        self.continue_run(request)
    }
}

impl ContinuationPort for FilesystemContinuation {
    fn continue_after_limit(&self, request: &ContinuationRequest) -> Result<ContinuationOutcome> {
        ContinuationService {
            rotation: &self.rotation,
            handoff: &self.handoff,
        }
        .continue_run(request)
    }
}
