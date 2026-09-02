use std::collections::BTreeMap;

use serde_json::Value;

pub fn json_object(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice(bytes).ok()
}

pub fn json_lines(text: &str) -> impl DoubleEndedIterator<Item = Value> + '_ {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

pub fn flatten_extra(
    object: &serde_json::Map<String, Value>,
    known: &[&str],
) -> BTreeMap<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
