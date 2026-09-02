use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use neomax_core::WorkerScope;
use neomax_core::accounts::{AccountInventory, AccountSelector, SelectionPolicy, select_account};
use neomax_core::runs::{RunRecord, RunStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RetrySelector {
    Auto,
    Account(String),
}

pub(crate) trait RetryAccountSelector: Send + Sync {
    fn select(
        &self,
        run: &RunRecord,
        selector: &RetrySelector,
        excluded: &BTreeSet<PathBuf>,
    ) -> neomax_core::Result<PathBuf>;
}

pub(crate) struct InventoryRetrySelector<'a> {
    pub inventory: &'a AccountInventory<'a>,
    pub scope: &'a WorkerScope,
    pub now: DateTime<Utc>,
    pub policy: SelectionPolicy,
}

impl RetryAccountSelector for InventoryRetrySelector<'_> {
    fn select(
        &self,
        run: &RunRecord,
        selector: &RetrySelector,
        excluded: &BTreeSet<PathBuf>,
    ) -> neomax_core::Result<PathBuf> {
        let accounts = self.inventory.routing_snapshots(self.scope, self.now)?;
        let selector = match selector {
            RetrySelector::Auto => AccountSelector::Auto,
            RetrySelector::Account(account) => AccountSelector::Account(account.clone()),
        };
        let decision = select_account(
            &accounts,
            &selector,
            excluded,
            &Default::default(),
            self.now,
            &self.policy,
        )?;
        if decision.account.engine != run.engine {
            return Err(neomax_core::Error::Conflict(format!(
                "retry target {} belongs to {}, expected {}",
                decision.account.account, decision.account.engine, run.engine
            )));
        }
        Ok(decision.account.profile.clone())
    }
}

pub(crate) trait RunExecutor: Send + Sync {
    fn execute(&self, run: &mut RunRecord) -> neomax_core::Result<RunStatus>;
}

impl RunExecutor for neomax_core::runs::coordinator::RunCoordinator<'_> {
    fn execute(&self, run: &mut RunRecord) -> neomax_core::Result<RunStatus> {
        Ok(neomax_core::runs::coordinator::RunCoordinator::execute(self, run)?.status)
    }
}
