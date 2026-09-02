use anyhow::Result;
use neomax_core::scheduler::persistence::PlanStore;
use serde_json::Value;

use super::FilesystemPortalSource;

pub(crate) fn read_plans(source: &FilesystemPortalSource) -> Result<(Vec<Value>, usize)> {
    let view = PlanStore::new(source.paths.plans.clone()).all_with_diagnostics()?;
    let mut skipped = view.diagnostics.len();
    let records = view
        .records
        .into_iter()
        .filter_map(|record| match serde_json::to_value(record) {
            Ok(value) => Some(value),
            Err(_) => {
                skipped = skipped.saturating_add(1);
                None
            }
        })
        .collect();
    Ok((records, skipped))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use neomax_core::scheduler::persistence::{PlanRecord, PlanStore};

    #[test]
    fn reads_valid_plans_and_reports_corrupt_optional_records() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        fs::create_dir_all(&source.paths.plans).unwrap();
        fs::write(source.paths.plans.join("broken.json"), b"{").unwrap();

        let plan = neomax_core::scheduler::Plan::from_parts(vec![neomax_core::scheduler::Part {
            id: "part-1".into(),
            prompt: "build".into(),
            engine: neomax_core::Engine::Claude,
            model: None,
            area: Default::default(),
            depends_on: Default::default(),
            effort: None,
            ultra: false,
            opus: false,
            codex_model: None,
            kimi_model: None,
            order: 0,
            extra: Default::default(),
        }])
        .unwrap();
        let record = PlanRecord::new("plan-1", plan, None, 1).unwrap();
        PlanStore::new(&source.paths.plans).create(&record).unwrap();

        let (records, skipped) = read_plans(&source).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["plan_id"], "plan-1");
        assert_eq!(skipped, 1);
    }

    #[test]
    fn missing_plan_directory_is_an_empty_optional_view() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let (records, skipped) = read_plans(&source).unwrap();
        assert!(records.is_empty());
        assert_eq!(skipped, 0);
    }
}
