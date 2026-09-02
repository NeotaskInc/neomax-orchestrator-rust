use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::Engine;
use crate::accounts::{AccountSnapshot, FIVE_HOUR_SOFT_PERCENT, engine_has_five_hour};

use super::account::choose_provider_orchestrator;
use super::types::{NeomaxChoice, NeomaxSelectionRequest, ProviderSelectionRequest};

pub fn choose_neomax_orchestrator(request: NeomaxSelectionRequest<'_>) -> Option<NeomaxChoice> {
    let worker_engines = request
        .priority
        .iter()
        .copied()
        .filter(|engine| choose_provider(&request, *engine, false).is_some())
        .collect::<Vec<_>>();
    let candidates = request
        .priority
        .iter()
        .copied()
        .filter_map(|engine| {
            choose_provider(&request, engine, request.dedicated).map(|account| (engine, account))
        })
        .collect::<BTreeMap<_, _>>();

    let (account, reason) = if let Some(engine) = request.forced_engine {
        (
            candidates.get(&engine)?.clone(),
            format!("explicit --engine {engine}"),
        )
    } else if request.resume {
        let engine = request
            .previous_engine
            .filter(|engine| candidates.contains_key(engine))?;
        (
            candidates.get(&engine)?.clone(),
            "resume uses this project's previous Neomax orchestrator".into(),
        )
    } else {
        let rank = request
            .priority
            .iter()
            .enumerate()
            .map(|(index, engine)| (*engine, index))
            .collect::<BTreeMap<_, _>>();
        let account = candidates
            .values()
            .min_by_key(|account| {
                provider_score(account, request.previous_engine, &rank, request.now)
            })?
            .clone();
        let reason = if usage_pressure(&account, request.now).is_some() {
            "largest measured quota headroom among healthy eligible providers".into()
        } else {
            "best eligible provider by live load and recent selection; quota percentage unavailable"
                .into()
        };
        (account, reason)
    };
    Some(NeomaxChoice {
        engine: account.engine,
        profile: account.profile.clone(),
        pressure: usage_pressure(&account, request.now),
        live: account.live_workers,
        worker_engines,
        orchestrator_engines: request
            .priority
            .iter()
            .copied()
            .filter(|engine| candidates.contains_key(engine))
            .collect(),
        reason,
        priority: request.priority.to_vec(),
        cwd: request.cwd,
    })
}

fn choose_provider(
    request: &NeomaxSelectionRequest<'_>,
    engine: Engine,
    dedicated: bool,
) -> Option<AccountSnapshot> {
    choose_provider_orchestrator(&ProviderSelectionRequest {
        accounts: request.accounts,
        orchestrators: request.orchestrators,
        engine,
        dedicated,
        current_session: request.current_session,
        now: request.now,
        policy: request.policy,
    })
}

fn provider_score(
    account: &AccountSnapshot,
    previous_engine: Option<Engine>,
    rank: &BTreeMap<Engine, usize>,
    now: DateTime<Utc>,
) -> (u8, OrderedFloat, u32, u8, usize) {
    let pressure = usage_pressure(account, now);
    let tier = match pressure {
        Some(value) if value < FIVE_HOUR_SOFT_PERCENT => 0,
        None => 1,
        Some(_) => 2,
    };
    (
        tier,
        OrderedFloat(pressure.unwrap_or(0.0)),
        account.live_workers,
        u8::from(previous_engine == Some(account.engine)),
        rank.get(&account.engine).copied().unwrap_or(usize::MAX),
    )
}

fn usage_pressure(account: &AccountSnapshot, now: DateTime<Utc>) -> Option<f64> {
    let mut values = Vec::new();
    if engine_has_five_hour(account.engine) && account.five_hour_percent.is_some() {
        values.push(account.five_hour_at(now));
    }
    if account.weekly_percent.is_some() {
        values.push(account.weekly_at(now));
    }
    values.into_iter().max_by(f64::total_cmp)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedFloat(f64);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;

    use super::*;
    use crate::orchestration::selection::OrchestratorPolicy;

    fn account(
        engine: Engine,
        name: &str,
        five: Option<f64>,
        weekly: Option<f64>,
    ) -> AccountSnapshot {
        AccountSnapshot {
            engine,
            account: name.into(),
            profile: PathBuf::from(format!("/profiles/.{}{}", engine.as_str(), name)),
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

    fn request<'a>(
        accounts: &'a [AccountSnapshot],
        priority: &'a [Engine],
        now: DateTime<Utc>,
        policy: &'a OrchestratorPolicy,
    ) -> NeomaxSelectionRequest<'a> {
        NeomaxSelectionRequest {
            accounts,
            orchestrators: &[],
            priority,
            forced_engine: None,
            cwd: "/workspace".into(),
            resume: false,
            dedicated: false,
            previous_engine: None,
            current_session: None,
            now,
            policy,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).single().unwrap()
    }

    #[test]
    fn selects_measured_headroom_across_connected_providers() {
        let now = now();
        let accounts = [
            account(Engine::Claude, "1", Some(60.0), Some(80.0)),
            account(Engine::Codex, "1", None, Some(20.0)),
            account(Engine::Kimi, "1", None, None),
        ];
        let priority = [Engine::Claude, Engine::Codex, Engine::Kimi];
        let policy = OrchestratorPolicy::default();
        let choice =
            choose_neomax_orchestrator(request(&accounts, &priority, now, &policy)).unwrap();
        assert_eq!(choice.engine, Engine::Codex);
        assert_eq!(choice.worker_engines, priority);
    }

    #[test]
    fn resume_and_forced_modes_honor_project_and_user_choice() {
        let now = now();
        let accounts = [
            account(Engine::Claude, "1", Some(80.0), Some(80.0)),
            account(Engine::Codex, "1", None, Some(10.0)),
        ];
        let priority = [Engine::Claude, Engine::Codex];
        let policy = OrchestratorPolicy::default();
        let mut resumed = request(&accounts, &priority, now, &policy);
        resumed.resume = true;
        resumed.previous_engine = Some(Engine::Claude);
        assert_eq!(
            choose_neomax_orchestrator(resumed).unwrap().engine,
            Engine::Claude
        );

        let mut forced = request(&accounts, &priority, now, &policy);
        forced.forced_engine = Some(Engine::Claude);
        assert_eq!(
            choose_neomax_orchestrator(forced).unwrap().engine,
            Engine::Claude
        );
    }

    #[test]
    fn hard_wall_scope_excludes_only_that_provider_and_keeps_other_workers_available() {
        let now = now();
        let accounts = [
            account(Engine::Claude, "1", Some(99.0), Some(10.0)),
            account(Engine::Codex, "1", None, Some(10.0)),
            account(Engine::Opencode, "1", None, None),
        ];
        let priority = [Engine::Claude, Engine::Codex, Engine::Opencode];
        let policy = OrchestratorPolicy::default();
        let choice =
            choose_neomax_orchestrator(request(&accounts, &priority, now, &policy)).unwrap();
        assert_eq!(choice.engine, Engine::Codex);
        assert_eq!(choice.worker_engines, vec![Engine::Codex, Engine::Opencode]);
        assert_eq!(
            choice.orchestrator_engines,
            vec![Engine::Codex, Engine::Opencode]
        );
    }
}
