use serde_json::Value;

pub(super) fn stream(rows: &[Value]) -> Vec<u8> {
    rows.iter()
        .map(|row| format!("{}\n", serde_json::to_string(row).unwrap()))
        .collect::<String>()
        .into_bytes()
}
