use std::path::{Path, PathBuf};

use crate::atomic::{read_json_or_default, update_json_locked, with_exclusive_lock};
use crate::{Result, atomic::write_json_atomic};

use super::policy::SelfHealPolicy;
use super::types::{HealDecision, RepairAction, SelfHealEvent, SelfHealRecord, SelfHealState};

#[derive(Debug, Clone)]
pub struct SelfHealStore {
    path: PathBuf,
    lock: PathBuf,
}

impl SelfHealStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let lock = PathBuf::from(format!("{}.lock", path.display()));
        Self { path, lock }
    }

    pub fn at(path: impl Into<PathBuf>, lock: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: lock.into(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> SelfHealState {
        read_json_or_default(&self.path)
    }

    pub fn save(&self, state: &SelfHealState) -> Result<()> {
        with_exclusive_lock(&self.lock, || write_json_atomic(&self.path, state))
    }

    pub fn decision(
        &self,
        run_id: &str,
        now: i64,
        policy: &SelfHealPolicy,
        allow_repeat: bool,
    ) -> HealDecision {
        let state = self.load();
        decision_for(state.runs.get(run_id), now, policy, allow_repeat)
    }

    pub fn reserve(
        &self,
        run_id: &str,
        action: RepairAction,
        now: i64,
        policy: &SelfHealPolicy,
        allow_repeat: bool,
    ) -> Result<HealDecision> {
        let mut decision = HealDecision::CapReached;
        update_json_locked::<SelfHealState, _>(&self.path, &self.lock, |state| {
            let entry = state.runs.entry(run_id.to_owned()).or_default();
            decision = decision_for(Some(entry), now, policy, allow_repeat);
            if let HealDecision::Eligible { attempt, next_at } = decision {
                entry.attempts = attempt;
                entry.last_at = Some(now);
                entry.next_at = Some(next_at);
                entry.push_event(SelfHealEvent::new(now, action, true), policy.max_history);
            }
            Ok(())
        })?;
        Ok(decision)
    }

    pub fn complete(
        &self,
        run_id: &str,
        action: RepairAction,
        now: i64,
        outcome: &str,
    ) -> Result<()> {
        update_json_locked::<SelfHealState, _>(&self.path, &self.lock, |state| {
            let entry = state.runs.entry(run_id.to_owned()).or_default();
            if let Some(event) = entry
                .history
                .iter_mut()
                .rev()
                .find(|event| event.in_flight && event.action == action.as_str())
            {
                event.in_flight = false;
                event.outcome = Some(outcome.to_owned());
                event.ts = now;
            } else {
                let mut event = SelfHealEvent::new(now, action, false);
                event.outcome = Some(outcome.to_owned());
                entry.push_event(event, 32);
            }
            entry.last_at = Some(now);
            Ok(())
        })?;
        Ok(())
    }
}

fn decision_for(
    entry: Option<&SelfHealRecord>,
    now: i64,
    policy: &SelfHealPolicy,
    allow_repeat: bool,
) -> HealDecision {
    let Some(entry) = entry else {
        let attempt = 1;
        return HealDecision::Eligible {
            attempt,
            next_at: now.saturating_add(policy.backoff_seconds(attempt)),
        };
    };
    if entry.attempts >= policy.max_attempts {
        return HealDecision::CapReached;
    }
    if entry.next_at.is_some_and(|next_at| next_at > now) {
        return HealDecision::Backoff {
            next_at: entry.next_at.unwrap_or(now),
        };
    }
    if !allow_repeat && entry.attempts > 0 {
        return HealDecision::AlreadyHealed;
    }
    let attempt = entry.attempts.saturating_add(1);
    HealDecision::Eligible {
        attempt,
        next_at: now.saturating_add(policy.backoff_seconds(attempt)),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn reserves_once_and_persists_legacy_top_level_shape() {
        let temp = tempfile::tempdir().unwrap();
        let store = SelfHealStore::new(temp.path().join("self-heal.json"));
        let policy = SelfHealPolicy::default();
        assert!(matches!(
            store.reserve("run-1", RepairAction::Retry, 100, &policy, false),
            Ok(HealDecision::Eligible { attempt: 1, .. })
        ));
        assert!(matches!(
            store.reserve("run-1", RepairAction::Retry, 100, &policy, false),
            Ok(HealDecision::Backoff { .. })
        ));
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains("run-1"));
        assert!(!raw.contains("/"));
    }

    #[test]
    fn unknown_top_level_and_record_fields_survive_restart() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("self-heal.json");
        fs::write(
            &path,
            r#"{
                "run-1":{"attempts":1,"history":[],"future_record":{"keep":true}},
                "future_root":{"version":2}
            }"#,
        )
        .unwrap();
        let store = SelfHealStore::new(&path);
        let state = store.load();
        assert_eq!(state.runs["run-1"].extra["future_record"]["keep"], true);
        assert_eq!(state.extra["future_root"]["version"], 2);
        store
            .complete("run-1", RepairAction::Retry, 110, "completed")
            .unwrap();
        let state = store.load();
        assert_eq!(state.extra["future_root"]["version"], 2);
        assert_eq!(state.runs["run-1"].extra["future_record"]["keep"], true);
    }

    #[test]
    fn future_wrapped_schema_keeps_its_shape_and_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("self-heal.json");
        fs::write(
            &path,
            r#"{
                "version":2,
                "runs":{"run-1":{"attempts":1,"history":[],"future":true}},
                "future_root":"keep"
            }"#,
        )
        .unwrap();
        let store = SelfHealStore::new(&path);
        let state = store.load();
        store.save(&state).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(value["runs"]["run-1"].is_object());
        assert_eq!(value["runs"]["run-1"]["future"], true);
        assert_eq!(value["future_root"], "keep");
    }

    #[test]
    fn cap_is_durable_and_allow_repeat_cannot_escape_it() {
        let temp = tempfile::tempdir().unwrap();
        let store = SelfHealStore::new(temp.path().join("self-heal.json"));
        let policy = SelfHealPolicy {
            max_attempts: 2,
            initial_backoff: std::time::Duration::ZERO,
            max_backoff: std::time::Duration::ZERO,
            ..SelfHealPolicy::default()
        };
        assert!(matches!(
            store.reserve("run-1", RepairAction::Retry, 1, &policy, false),
            Ok(HealDecision::Eligible { attempt: 1, .. })
        ));
        store
            .complete("run-1", RepairAction::Retry, 1, "failed")
            .unwrap();
        assert!(matches!(
            store.reserve("run-1", RepairAction::Retry, 2, &policy, true),
            Ok(HealDecision::Eligible { attempt: 2, .. })
        ));
        store
            .complete("run-1", RepairAction::Retry, 2, "failed")
            .unwrap();
        assert_eq!(
            store.decision("run-1", 3, &policy, true),
            HealDecision::CapReached
        );
    }
}
