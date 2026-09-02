use crate::Result;
use crate::config::Engine;

use super::store::DispatchAdmissionStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionLease {
    pub id: String,
    pub(super) store: DispatchAdmissionStore,
}

impl AdmissionLease {
    pub fn bind(
        &self,
        engine: Engine,
        account: impl Into<String>,
        session: impl Into<String>,
    ) -> Result<()> {
        self.store
            .bind(&self.id, engine, account.into(), session.into())
    }

    pub fn release(self) -> Result<()> {
        self.store.release(&self.id).map(|_| ())
    }
}

impl Drop for AdmissionLease {
    fn drop(&mut self) {
        let _ = self.store.release(&self.id);
    }
}
