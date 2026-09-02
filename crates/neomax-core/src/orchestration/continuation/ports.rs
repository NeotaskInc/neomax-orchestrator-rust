use std::path::Path;

use crate::orchestration::auth::{CredentialWriter, RotationEffects, RotationService};
use crate::orchestration::handoff::{HandoffBaton, HandoffStore};
use crate::Engine;
use crate::Result;

pub trait CredentialRotationPort: Send + Sync {
    fn supports(&self, engine: Engine) -> bool;

    fn swap(
        &self,
        engine: Engine,
        destination: &Path,
        source: &Path,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects>;

    fn rollback(
        &self,
        effects: &RotationEffects,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<()> {
        let source = effects.source.as_deref().ok_or_else(|| {
            crate::Error::InvalidArgument("rotation effects have no source profile".into())
        })?;
        self.swap(
            effects.engine,
            source,
            &effects.destination,
            timestamp,
            reason,
        )
        .map(|_| ())
    }
}

impl<W: CredentialWriter> CredentialRotationPort for RotationService<W> {
    fn supports(&self, engine: Engine) -> bool {
        crate::orchestration::auth::copy_allowed(engine).is_ok()
    }

    fn swap(
        &self,
        engine: Engine,
        destination: &Path,
        source: &Path,
        timestamp: i64,
        reason: Option<String>,
    ) -> Result<RotationEffects> {
        RotationService::swap(self, engine, destination, source, timestamp, reason)
    }
}

pub trait HandoffPort: Send + Sync {
    fn save(&self, baton: &HandoffBaton) -> Result<()>;
}

impl HandoffPort for HandoffStore {
    fn save(&self, baton: &HandoffBaton) -> Result<()> {
        HandoffStore::save(self, baton)
    }
}
