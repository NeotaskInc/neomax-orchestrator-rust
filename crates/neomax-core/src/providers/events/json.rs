use serde_json::Value;

pub(super) fn json_lines(bytes: &[u8]) -> impl Iterator<Item = Value> + '_ {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object)
        .collect::<Vec<_>>()
        .into_iter()
}

pub(super) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(super) fn status_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

pub(super) fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

pub(super) fn u64_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn json_lines_keeps_object_rows_and_discards_other_rows() {
        let rows = json_lines(
            br#"{"ok":true}
not-json
42
{"done":false}
"#,
        )
        .collect::<Vec<_>>();

        assert_eq!(rows, vec![json!({"ok": true}), json!({"done": false})]);
    }

    #[test]
    fn scalar_helpers_preserve_provider_event_coercions() {
        let value = json!({
            "text": "ready",
            "number": 2.5,
            "number_string": "3.5",
            "count": 7,
            "count_string": "8"
        });

        assert_eq!(string_field(&value, "text").as_deref(), Some("ready"));
        assert_eq!(number_value(value.get("number").unwrap()), Some(2.5));
        assert_eq!(number_value(value.get("number_string").unwrap()), Some(3.5));
        assert_eq!(u64_value(value.get("count").unwrap()), Some(7));
        assert_eq!(u64_value(value.get("count_string").unwrap()), Some(8));
        assert_eq!(status_string(&Value::Null), "");
        assert_eq!(
            status_string(&json!({"state": "ready"})),
            r#"{"state":"ready"}"#
        );
    }
}
