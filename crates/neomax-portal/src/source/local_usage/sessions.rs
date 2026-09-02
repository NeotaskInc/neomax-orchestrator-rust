use neomax_core::config::Engine;
use neomax_core::providers::ProviderProfile;
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::{
    LocalUsageEntry, LocalUsageSnapshot, ProviderUsageDetail, build_provider_usage_detail,
};

use super::accounting::{FileTotals, children_metrics, entry};
use super::errors::update_session_error;
use super::metrics::subtract;
use super::tools::from_counts;

pub(crate) fn detail(
    engine: Engine,
    profile: &ProviderProfile,
    days: u32,
    records: &[SessionRecord],
    cutoff: i64,
) -> ProviderUsageDetail {
    debug_assert!(matches!(engine, Engine::Kimi | Engine::Grok));
    let matching = records
        .iter()
        .filter(|record| record.engine == engine && record.account == profile.account)
        .filter(|record| record.last_active.unwrap_or_default() >= cutoff)
        .collect::<Vec<_>>();
    let main = matching
        .iter()
        .filter(|record| !record.is_child())
        .copied()
        .collect::<Vec<_>>();
    let children = matching
        .iter()
        .filter(|record| record.is_child())
        .copied()
        .collect::<Vec<_>>();
    let mut snapshot = LocalUsageSnapshot {
        available: profile.path.is_dir(),
        source: match engine {
            Engine::Kimi => "kimi-local-wire".into(),
            Engine::Grok => "grok-local-jsonl".into(),
            _ => unreachable!(),
        },
        account: profile.account.clone(),
        window_days: days,
        sessions: if engine == Engine::Kimi {
            main.len() as u64
        } else {
            matching.len() as u64
        },
        main_sessions: main.len() as u64,
        native_subagents: children.len() as u64,
        ..LocalUsageSnapshot::default()
    };
    snapshot.last_activity = matching
        .iter()
        .filter_map(|record| record.last_active)
        .max()
        .unwrap_or_default();
    let mut tool_calls = 0_u64;
    let mut tool_errors = 0_u64;
    let mut files = FileTotals::new();
    for record in &main {
        let full = entry(record);
        update_session_error(&mut snapshot.last_error, record);
        let residual = subtract(&full.metrics, &children_metrics(&children, &record.id));
        snapshot.entries.push(LocalUsageEntry {
            model: None,
            agent: None,
            metrics: full.metrics,
        });
        snapshot.model_entries.push(LocalUsageEntry {
            model: record.model.clone(),
            agent: None,
            metrics: residual,
        });
        tool_calls = tool_calls.saturating_add(record.tool_calls);
        tool_errors = tool_errors.saturating_add(record.tool_errors);
        files.include(record);
    }
    for record in &children {
        let child_entry = entry(record);
        update_session_error(&mut snapshot.last_error, record);
        if engine == Engine::Kimi {
            snapshot.model_entries.push(child_entry.clone());
        } else {
            tool_calls = tool_calls.saturating_add(record.tool_calls);
            tool_errors = tool_errors.saturating_add(record.tool_errors);
            files.include(record);
        }
        snapshot.agent_entries.push(LocalUsageEntry {
            agent: Some(record.label.clone().unwrap_or_else(|| record.id.clone())),
            ..child_entry
        });
    }
    snapshot.files = files.paths.len() as u64;
    snapshot.adds = files.adds;
    snapshot.dels = files.dels;
    snapshot.tool_calls = Some(tool_calls);
    snapshot.tool_errors = Some(tool_errors);
    snapshot.tool_usage = from_counts(tool_calls, tool_errors);
    build_provider_usage_detail(snapshot)
}
