use crate::Result;

use super::schema::EffectiveSettings;

impl EffectiveSettings {
    pub fn default_run_all_capacity(&self, eligible_accounts: usize) -> usize {
        let per_account = self
            .concurrency
            .lanes_per_account
            .min(self.concurrency.max_sessions_per_account) as usize;
        let mut capacity = eligible_accounts.saturating_mul(per_account);
        capacity = capacity.min(self.concurrency.max_subagents as usize);
        if self.concurrency.max_tasks != 0 {
            capacity = capacity.min(self.concurrency.max_tasks as usize);
        }
        if let Some(fleet_cap) = self.concurrency.fleet_live_cap {
            capacity = capacity.min(fleet_cap as usize);
        }
        capacity
    }

    pub fn validate_run_all_capacity(
        &self,
        requested: usize,
        eligible_accounts: usize,
    ) -> Result<()> {
        let capacity = self.default_run_all_capacity(eligible_accounts);
        if requested > capacity {
            return Err(crate::Error::InvalidArgument(format!(
                concat!(
                    "scheduler max_live {} exceeds effective run-all capacity {} ",
                    "(accounts={}, lanes_per_account={}, ",
                    "max_sessions_per_account={}, max_subagents={}, max_tasks={}, ",
                    "fleet_live_cap={})"
                ),
                requested,
                capacity,
                eligible_accounts,
                self.concurrency.lanes_per_account,
                self.concurrency.max_sessions_per_account,
                self.concurrency.max_subagents,
                self.concurrency.max_tasks,
                self.concurrency
                    .fleet_live_cap
                    .map_or_else(|| "none".to_owned(), |value| value.to_string()),
            )));
        }
        Ok(())
    }
}
