use std::collections::BTreeMap;

use chrono::DateTime;
use serde_json::Value;

use super::QuotaWindow;

pub use crate::accounts::{claude_model_family, claude_limit_family};

pub fn claude_model_windows(response: &Value) -> BTreeMap<String, QuotaWindow> {
    let response = response.get("rate_limits").unwrap_or(response);
    let mut windows = BTreeMap::new();
    if let Some(value) = response.get("seven_day_overage_included") {
        insert(&mut windows, "fable", value.get("utilization"), value.get("resets_at"));
    }
    for family in ["opus", "sonnet"] {
        if let Some(value) = response.get(format!("seven_day_{family}")) {
            insert(&mut windows, family, value.get("utilization"), value.get("resets_at"));
        }
    }
    if let Some(limits) = response.get("limits").and_then(Value::as_array) {
        for limit in limits {
            if limit.get("kind").and_then(Value::as_str) != Some("weekly_scoped") {
                continue;
            }
            if let Some(family) = limit.pointer("/scope/model/display_name")
                .and_then(Value::as_str).and_then(claude_model_family)
            {
                insert(&mut windows, family, limit.get("percent"), limit.get("resets_at"));
            }
        }
    }
    if let Some(limits) = response.get("model_scoped").and_then(Value::as_array) {
        for limit in limits {
            if let Some(family) = limit.get("display_name").and_then(Value::as_str)
                .and_then(claude_model_family)
            {
                insert(&mut windows, family, limit.get("utilization"), limit.get("resets_at"));
            }
        }
    }
    windows
}

fn insert(windows: &mut BTreeMap<String, QuotaWindow>, family: &str, percent: Option<&Value>, reset: Option<&Value>) {
    let used = percent.and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|v: &f64| v.is_finite() && (0.0..=100.0).contains(v));
    let Some(used) = used else { return };
    let resets_at = reset.and_then(|v| v.as_f64().or_else(|| {
        DateTime::parse_from_rfc3339(v.as_str()?).ok().map(|v| v.timestamp() as f64)
    })).filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| if v >= 1_000_000_000_000.0 { v / 1000.0 } else { v });
    let window = QuotaWindow { used_percent: Some(used), resets_at };
    if windows.get(family).is_none_or(|existing| existing.used_percent.unwrap_or(0.0) < used) {
        windows.insert(family.into(), window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_cli_weekly_scoped_and_structured_usage_without_confusing_credits() {
        let raw = claude_model_windows(&json!({
            "limits": [
                {"kind":"weekly_scoped","percent":100,"resets_at":"2040-01-01T00:00:00Z","scope":{"model":{"display_name":"Fable"}}},
                {"kind":"daily_scoped","percent":100,"scope":{"model":{"display_name":"Opus"}}}
            ],
            "cinder_cove": {"utilization":100},
            "extra_usage": {"utilization":100}
        }));
        assert_eq!(raw.len(), 1);
        assert_eq!(raw["fable"].used_percent, Some(100.0));
        let structured = claude_model_windows(&json!({"rate_limits":{"model_scoped":[
            {"display_name":"Fable", "utilization":100,"resets_at":2208988800000_u64}
        ]}}));
        assert_eq!(raw, structured);
        assert!(claude_model_windows(&json!({"cinder_cove":{"utilization":100}})).is_empty());
    }

    #[test]
    fn scopes_aliases_and_keeps_legacy_model_windows() {
        for model in ["fable", "claude-fable-5[1m]", "claude-fable-5-1[1m]", "Fable 5.1"] {
            assert_eq!(claude_model_family(model), Some("fable"));
        }
        assert_eq!(claude_model_family("gpt-6-astra"), None);
        let windows = claude_model_windows(&json!({
            "seven_day_overage_included":{"utilization":99},
            "seven_day_opus":{"utilization":12},
            "seven_day_sonnet":{"utilization":150}
        }));
        assert_eq!(windows.len(), 2);
        assert_eq!(windows["fable"].used_percent, Some(99.0));
        assert_eq!(windows["opus"].used_percent, Some(12.0));
    }
}
