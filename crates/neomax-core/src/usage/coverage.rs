use std::path::Path;

use super::UsageReport;

pub fn append_import_warnings(report: &mut UsageReport, state_path: &Path) {
    let Ok(state) = crate::atomic::read_json::<serde_json::Value>(state_path) else {
        if state_path.exists() {
            report.warnings.push("Local import status could not be read; source completeness is unverified.".into());
        }
        return;
    };
    let Some(import) = state.get("usage_import") else {
        report.warnings.push("Local history has not yet completed the source-coverage check.".into());
        return;
    };
    let files = import.get("pending_files").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let bytes = import.get("pending_bytes").and_then(serde_json::Value::as_u64).unwrap_or(0);
    let errors = import.get("errors").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if files > 0 {
        report.warnings.push(format!("Local history import is incomplete: {files} files have {bytes} unread bytes. Totals will change as the importer catches up."));
    }
    if errors > 0 {
        report.warnings.push(format!("The last local import reported {errors} read or parse errors; usage may be incomplete."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_unfinished_import_and_clears_after_completion() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("watch.json");
        for pending in [3, 0] {
            crate::atomic::write_json_atomic(&path, &serde_json::json!({"usage_import":{"pending_files":pending,"pending_bytes":pending*100,"errors":0}})).unwrap();
            let mut report = super::super::build_usage_report(&[], 30, 1000, &super::super::PriceCatalog::default());
            append_import_warnings(&mut report, &path);
            assert_eq!(report.warnings.len(), usize::from(pending > 0));
        }
    }
}
