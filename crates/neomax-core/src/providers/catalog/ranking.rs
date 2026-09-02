use std::cmp::Ordering;

use crate::Engine;

use super::types::OrchestratorCandidate;

pub const DEFAULT_NEOMAX_PRIORITY: [Engine; 5] = [
    Engine::Claude,
    Engine::Codex,
    Engine::Kimi,
    Engine::Grok,
    Engine::Opencode,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankingPolicy {
    pub hard_wall_percent: f64,
    pub measured_preference_ceiling: f64,
    pub live_weight: f64,
}

impl Default for RankingPolicy {
    fn default() -> Self {
        Self {
            hard_wall_percent: 99.0,
            measured_preference_ceiling: 90.0,
            live_weight: 1.0,
        }
    }
}

pub fn rank_neomax(
    candidates: impl IntoIterator<Item = OrchestratorCandidate>,
    priority: &[Engine],
    policy: RankingPolicy,
) -> Vec<OrchestratorCandidate> {
    let mut candidates = candidates
        .into_iter()
        .filter(|candidate| {
            candidate
                .pressure
                .is_none_or(|pressure| pressure < policy.hard_wall_percent)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| compare(left, right, priority, policy));
    candidates
}

pub fn choose_neomax(
    candidates: impl IntoIterator<Item = OrchestratorCandidate>,
    priority: &[Engine],
    policy: RankingPolicy,
) -> Option<OrchestratorCandidate> {
    rank_neomax(candidates, priority, policy).into_iter().next()
}

fn compare(
    left: &OrchestratorCandidate,
    right: &OrchestratorCandidate,
    priority: &[Engine],
    policy: RankingPolicy,
) -> Ordering {
    rank_key(left, priority, policy).cmp(&rank_key(right, priority, policy))
}

fn rank_key(
    candidate: &OrchestratorCandidate,
    priority: &[Engine],
    policy: RankingPolicy,
) -> (u8, OrderedFloat, OrderedFloat, u8, usize, Engine, String) {
    let tier = match candidate.pressure {
        Some(value) if value < policy.measured_preference_ceiling => 0,
        None => 1,
        Some(_) => 2,
    };
    let pressure = OrderedFloat(candidate.pressure.unwrap_or(0.0));
    let live = OrderedFloat(f64::from(candidate.live_workers) * policy.live_weight);
    let priority_rank = priority
        .iter()
        .position(|engine| *engine == candidate.engine)
        .unwrap_or(usize::MAX);
    (
        tier,
        pressure,
        live,
        u8::from(candidate.previous),
        priority_rank,
        candidate.engine,
        candidate.account.clone(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedFloat(f64);

impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
