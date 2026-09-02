use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::providers::runtime::ProviderRuntime;
use neomax_core::sessions::SessionRecord;

use crate::context::RuntimeContext;

use super::super::discovery::{DiscoveryOptions, MAX_DISCOVERY_DAYS, SessionInventory, discover};
use super::model::ResumeTarget;
use super::ownership::{ensure_owner_available, owner_available};

pub(crate) fn resolve_target(
    context: &RuntimeContext,
    selector: Option<&str>,
) -> Result<ResumeTarget> {
    resolve_target_with_filter(context, None, selector)
}

pub(crate) fn resolve_target_for_engine(
    context: &RuntimeContext,
    engine: Engine,
    selector: Option<&str>,
) -> Result<ResumeTarget> {
    resolve_target_with_filter(context, Some(engine), selector)
}

fn resolve_target_with_filter(
    context: &RuntimeContext,
    engine: Option<Engine>,
    selector: Option<&str>,
) -> Result<ResumeTarget> {
    let options = DiscoveryOptions {
        days: MAX_DISCOVERY_DAYS.saturating_add(1),
        limit: usize::MAX,
        ..DiscoveryOptions::default()
    };
    let mut inventory = discover(context, &options)?;
    if let Some(engine) = engine {
        inventory
            .all_records
            .retain(|record| record.engine == engine);
    }
    let runtime = context.provider_runtime()?;
    let latest_requested = selector.map(str::trim).is_none_or(str::is_empty)
        || (selector
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case("latest"))
            && !has_exact_id(&inventory, "latest"));
    let target = if latest_requested {
        select_latest_available(&inventory, &runtime)?
    } else {
        select_target(&inventory, selector)?
    };
    ensure_owner_available(&runtime, &inventory, &target)
}

fn select_target(inventory: &SessionInventory, selector: Option<&str>) -> Result<ResumeTarget> {
    let mut records = resumable_records(inventory);
    let selector = selector.map(str::trim).filter(|value| !value.is_empty());
    if let Some(selector) = selector {
        let exact = records
            .iter()
            .filter(|record| record.id == selector)
            .copied()
            .collect::<Vec<_>>();
        if exact.len() == 1 {
            return target_from_record(exact[0]);
        }
        if exact.len() > 1 {
            return ambiguous(selector, &exact);
        }
        if selector.eq_ignore_ascii_case("latest") {
            sort_by_recency(&mut records);
            return records
                .first()
                .copied()
                .ok_or_else(|| anyhow::anyhow!("no resumable sessions were discovered"))
                .and_then(target_from_record);
        }
        records.retain(|record| record.id.starts_with(selector));
        return match records.as_slice() {
            [] => bail!("no resumable session matches {selector:?}"),
            [record] => target_from_record(record),
            matches => ambiguous(selector, matches),
        };
    }

    sort_by_recency(&mut records);
    records
        .first()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("no resumable sessions were discovered"))
        .and_then(target_from_record)
}

fn resumable_records(inventory: &SessionInventory) -> Vec<&SessionRecord> {
    inventory
        .all_records
        .iter()
        .filter(|record| !record.is_child())
        .filter(|record| !record.id.trim().is_empty())
        .collect()
}

fn sort_by_recency(records: &mut [&SessionRecord]) {
    records.sort_by(|left, right| {
        right
            .last_active
            .unwrap_or_default()
            .cmp(&left.last_active.unwrap_or_default())
            .then_with(|| left.engine.cmp(&right.engine))
            .then_with(|| left.account.cmp(&right.account))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn has_exact_id(inventory: &SessionInventory, selector: &str) -> bool {
    inventory
        .all_records
        .iter()
        .any(|record| !record.is_child() && record.id == selector)
}

fn target_from_record(record: &SessionRecord) -> Result<ResumeTarget> {
    if record.id.trim().is_empty() {
        bail!("discovered session has no resumable ID");
    }
    Ok(ResumeTarget {
        engine: record.engine,
        account: record.account.clone(),
        session_id: record.id.clone(),
    })
}

fn ambiguous(selector: &str, records: &[&SessionRecord]) -> Result<ResumeTarget> {
    let choices = records
        .iter()
        .take(8)
        .map(|record| format!("{}:{}:{}", record.engine, record.account, record.id))
        .collect::<Vec<_>>();
    bail!(
        "session selector {selector:?} is ambiguous; matches {}{}",
        choices.join(", "),
        if records.len() > choices.len() {
            format!(" (and {} more)", records.len() - choices.len())
        } else {
            String::new()
        }
    )
}

fn select_latest_available(
    inventory: &SessionInventory,
    runtime: &ProviderRuntime,
) -> Result<ResumeTarget> {
    let mut records = resumable_records(inventory)
        .into_iter()
        .filter(|record| owner_available(inventory, runtime, record))
        .collect::<Vec<_>>();
    sort_by_recency(&mut records);
    records
        .first()
        .copied()
        .ok_or_else(|| {
            anyhow::anyhow!("no eligible authenticated resumable sessions were discovered")
        })
        .and_then(target_from_record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::Engine;
    use neomax_core::sessions::{SessionKind, SessionRecord};
    use std::collections::BTreeMap;

    fn record(id: &str, engine: Engine, account: &str, last_active: i64) -> SessionRecord {
        let mut record = SessionRecord::with_identity(id, engine, account);
        record.last_active = Some(last_active);
        record
    }

    fn inventory(records: impl IntoIterator<Item = SessionRecord>) -> SessionInventory {
        SessionInventory {
            all_records: records.into_iter().collect(),
            owners: BTreeMap::new(),
        }
    }

    #[test]
    fn exact_id_wins_over_prefix_candidates() {
        let target = select_target(
            &inventory([
                record("session", Engine::Claude, "1", 10),
                record("session-child", Engine::Codex, "2", 20),
            ]),
            Some("session"),
        )
        .unwrap();
        assert_eq!(target.session_id, "session");
        assert_eq!(target.engine, Engine::Claude);
    }

    #[test]
    fn unique_prefix_and_latest_work_across_engines() {
        let inventory = inventory([
            record("claude-old", Engine::Claude, "1", 10),
            record("codex-new", Engine::Codex, "2", 20),
        ]);
        assert_eq!(
            select_target(&inventory, Some("codex-n")).unwrap(),
            ResumeTarget {
                engine: Engine::Codex,
                account: "2".into(),
                session_id: "codex-new".into()
            }
        );
        assert_eq!(
            select_target(&inventory, None).unwrap().session_id,
            "codex-new"
        );
    }

    #[test]
    fn ambiguous_prefix_and_missing_id_fail_closed() {
        let inventory = inventory([
            record("same-a", Engine::Claude, "1", 10),
            record("same-b", Engine::Codex, "2", 20),
        ]);
        let ambiguous = select_target(&inventory, Some("same")).unwrap_err();
        assert!(ambiguous.to_string().contains("ambiguous"));
        let missing = select_target(&inventory, Some("unknown")).unwrap_err();
        assert!(missing.to_string().contains("no resumable session"));
    }

    #[test]
    fn child_sessions_are_not_resumable_root_targets() {
        let mut child = record("child", Engine::Kimi, "1", 10);
        child.kind = SessionKind::NativeSubagent;
        child.parent_id = Some("parent".into());
        let error = select_target(&inventory([child]), Some("child")).unwrap_err();
        assert!(error.to_string().contains("no resumable session"));
    }

    #[test]
    fn latest_keyword_selects_the_newest_session() {
        let target = select_target(
            &inventory([
                record("older", Engine::Claude, "1", 10),
                record("newer", Engine::Grok, "2", 20),
            ]),
            Some("latest"),
        )
        .unwrap();
        assert_eq!(target.session_id, "newer");
    }

    #[test]
    fn an_actual_latest_id_still_has_exact_id_priority() {
        let target = select_target(
            &inventory([
                record("latest", Engine::Claude, "1", 10),
                record("newer", Engine::Grok, "2", 20),
            ]),
            Some("latest"),
        )
        .unwrap();
        assert_eq!(target.session_id, "latest");
    }
}
