use serde_json::Value;

use neomax_core::usage::LocalToolUsageRow;

pub(crate) fn from_counts(tool_calls: u64, tool_errors: u64) -> Vec<LocalToolUsageRow> {
    let completed = tool_calls.saturating_sub(tool_errors);
    let mut rows = Vec::with_capacity(2);
    if completed > 0 {
        rows.push(LocalToolUsageRow {
            tool: "local".into(),
            status: "completed".into(),
            calls: completed,
        });
    }
    if tool_errors > 0 {
        rows.push(LocalToolUsageRow {
            tool: "local".into(),
            status: "error".into(),
            calls: tool_errors,
        });
    }
    rows
}

pub(crate) fn line_count(value: Option<&Value>) -> u64 {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| value.lines().count() as u64)
        .unwrap_or_default()
}
