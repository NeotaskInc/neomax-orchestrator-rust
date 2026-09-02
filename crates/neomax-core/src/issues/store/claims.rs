use std::collections::BTreeMap;

use crate::atomic::with_exclusive_lock;
use crate::Result;

use super::super::claims::{ClaimLiveness, ProcessLiveness};
use super::super::types::{Issue, IssueClaim, IssueEvent, IssueStatus};
use super::core::IssueStore;

impl IssueStore {
    pub fn claim<L, P>(
        &self,
        key: &str,
        session: Option<String>,
        pid: Option<u32>,
        now: i64,
        liveness: &L,
        processes: &P,
    ) -> Result<Option<Issue>>
    where
        L: ClaimLiveness,
        P: ProcessLiveness,
    {
        let lock = self.directory.join(".claim.lock");
        with_exclusive_lock(&lock, || {
            let Some(mut issue) = self.load(key)? else {
                return Ok(None);
            };
            let owner = session.clone().unwrap_or_else(|| {
                pid.map_or_else(|| "pid-unknown".into(), |pid| format!("pid-{pid}"))
            });
            if let Some(claim) = &issue.claim {
                let same_owner = claim.session == session && claim.pid == pid;
                if !same_owner && claim.is_active(now, self.config.claim_ttl, liveness, processes) {
                    return Ok(None);
                }
            }
            issue.claim = Some(IssueClaim::new(session, pid, now));
            if issue.status == IssueStatus::Open {
                issue.status = IssueStatus::Claimed;
            }
            let mut extra = BTreeMap::new();
            extra.insert("session".into(), owner.into());
            issue.history.push(IssueEvent {
                ts: now,
                event: "claimed".into(),
                extra: extra.clone(),
            });
            self.save_at(&mut issue, now)?;
            Ok(Some(issue))
        })
    }

    pub fn release(&self, key: &str, now: i64) -> Result<Option<Issue>> {
        let lock = self.directory.join(".claim.lock");
        with_exclusive_lock(&lock, || {
            let Some(mut issue) = self.load(key)? else {
                return Ok(None);
            };
            issue.claim = None;
            if matches!(issue.status, IssueStatus::Claimed | IssueStatus::Fixing) {
                issue.status = IssueStatus::Open;
            }
            issue.history.push(IssueEvent {
                ts: now,
                event: "released".into(),
                extra: BTreeMap::new(),
            });
            self.save_at(&mut issue, now)?;
            Ok(Some(issue))
        })
    }
}
