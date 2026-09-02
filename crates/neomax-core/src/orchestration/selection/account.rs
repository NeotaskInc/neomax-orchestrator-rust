use crate::accounts::{AccountSnapshot, rank_account};
use crate::orchestration::registry::OrchestratorRecord;

use super::types::ProviderSelectionRequest;

#[cfg(test)]
use super::types::OrchestratorPolicy;

pub fn choose_provider_orchestrator(
    request: &ProviderSelectionRequest<'_>,
) -> Option<AccountSnapshot> {
    request
        .accounts
        .iter()
        .filter(|account| account.engine == request.engine)
        .filter(|account| !request.dedicated || account.account == "orch")
        .filter(|account| {
            account.binary_available
                && account.authenticated
                && !account.paused
                && !account.at_hard_wall(request.now)
                && account
                    .cooldown_until
                    .is_none_or(|until| until <= request.now)
        })
        .min_by(|left, right| {
            account_score(left, request)
                .total_cmp(&account_score(right, request))
                .then_with(|| left.account.cmp(&right.account))
        })
        .cloned()
}

fn account_score(account: &AccountSnapshot, request: &ProviderSelectionRequest<'_>) -> f64 {
    let orchestrators_here = request
        .orchestrators
        .iter()
        .filter(|record| on_account(record, account, request.current_session))
        .count() as f64;
    let rank = rank_account(
        account,
        request.now,
        account.live_workers,
        &request.policy.account_ranking(),
    );
    orchestrators_here * request.policy.anti_stack_weight + rank.score + rank.weekly_percent
}

fn on_account(
    record: &OrchestratorRecord,
    account: &AccountSnapshot,
    current_session: Option<&str>,
) -> bool {
    record.live
        && record.engine == account.engine
        && record.account_dir == profile_name(&account.profile)
        && current_session.is_none_or(|session| record.session != session)
}

fn profile_name(path: &std::path::Path) -> &str {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::Engine;
    use chrono::Utc;

    use super::*;

    fn account(name: &str, five: Option<f64>, weekly: Option<f64>) -> AccountSnapshot {
        AccountSnapshot {
            engine: Engine::Claude,
            account: name.into(),
            profile: PathBuf::from(format!("/profiles/.claude{name}")),
            binary_available: true,
            authenticated: true,
            rotation_eligible: true,
            paused: false,
            reserved: false,
            live_workers: 0,
            five_hour_percent: five,
            weekly_percent: weekly,
            cooldown_until: None,
            five_hour_reset_at: None,
            weekly_reset_at: None,
        }
    }

    fn orchestrator(account_dir: &str) -> OrchestratorRecord {
        serde_json::from_value(serde_json::json!({
            "session":"other",
            "engine":"claude",
            "account_dir":account_dir,
            "last_seen":1,
            "live":true
        }))
        .unwrap()
    }

    #[test]
    fn avoids_stacking_and_rejects_the_hard_wall() {
        let now = Utc::now();
        let accounts = [
            account("full", Some(99.0), Some(99.0)),
            account("1", Some(0.0), Some(0.0)),
            account("2", Some(10.0), Some(0.0)),
        ];
        let records = [orchestrator(".claude1")];
        let selected = choose_provider_orchestrator(&ProviderSelectionRequest {
            accounts: &accounts,
            orchestrators: &records,
            engine: Engine::Claude,
            dedicated: false,
            current_session: None,
            now,
            policy: &OrchestratorPolicy::default(),
        })
        .unwrap();
        assert_eq!(selected.account, "2");
    }

    #[test]
    fn dedicated_mode_requires_the_orchestrator_profile() {
        let now = Utc::now();
        let mut normal = account("1", None, None);
        normal.engine = Engine::Kimi;
        let mut dedicated = account("orch", None, None);
        dedicated.engine = Engine::Kimi;
        let selected = choose_provider_orchestrator(&ProviderSelectionRequest {
            accounts: &[normal, dedicated],
            orchestrators: &[],
            engine: Engine::Kimi,
            dedicated: true,
            current_session: None,
            now,
            policy: &OrchestratorPolicy::default(),
        })
        .unwrap();
        assert_eq!(selected.account, "orch");
    }
}
