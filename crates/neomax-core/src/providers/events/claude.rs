use serde_json::Value;

use crate::providers::{ChildActivity, ParsedEvents, TokenUsage};

use super::common::{
    close_running, json_lines, number_value, status_string, string_field, u64_value, upsert,
};

pub fn parse(bytes: &[u8]) -> ParsedEvents {
    let mut output = ParsedEvents::default();
    for event in json_lines(bytes) {
        if output.session_id.is_none() {
            output.session_id = string_field(&event, "session_id").or_else(|| {
                event
                    .get("message")
                    .and_then(|message| string_field(message, "session_id"))
            });
        }
        match string_field(&event, "type").as_deref() {
            Some("rate_limit_event") => parse_rate_limit(&event, &mut output),
            Some("assistant") => {
                if string_field(&event, "error").as_deref() == Some("rate_limit") {
                    output.rate_limited = true;
                }
                parse_assistant(&event, &mut output);
            }
            Some("system") => parse_child(&event, &mut output.children),
            Some("result") => parse_result(&event, &mut output),
            _ => {}
        }
    }
    if output.subtype.as_deref() == Some("success") && !output.is_error {
        close_running(&mut output.children, "completed");
    }
    output
}

fn parse_rate_limit(event: &Value, output: &mut ParsedEvents) {
    let info = event
        .get("rate_limit_info")
        .or_else(|| event.get("rateLimitInfo"))
        .and_then(Value::as_object);
    if info
        .and_then(|item| item.get("status"))
        .and_then(Value::as_str)
        != Some("rejected")
    {
        return;
    }
    output.rate_limited = true;
    output.resets_at = info
        .and_then(|item| item.get("resetsAt").or_else(|| item.get("resets_at")))
        .and_then(super::common::number_value);
    output.limit_window = info
        .and_then(|item| {
            item.get("rateLimitType")
                .or_else(|| item.get("rate_limit_type"))
        })
        .and_then(Value::as_str)
        .map(str::to_string);
}

fn parse_assistant(event: &Value, output: &mut ParsedEvents) {
    let usage = event.get("usage").or_else(|| {
        event
            .get("message")
            .and_then(|message| message.get("usage"))
    });
    if let Some(usage) = usage {
        output.usage.add(&TokenUsage::from_value(usage));
    }
}

fn parse_result(event: &Value, output: &mut ParsedEvents) {
    if let Some(usage) = event.get("usage") {
        let usage = TokenUsage::from_value(usage);
        if usage_has_values(&usage) {
            output.usage = usage;
        }
    }
    if let Some(cost) = event
        .get("total_cost_usd")
        .or_else(|| event.get("totalCostUsd"))
        .and_then(number_value)
    {
        output.usage.cost = cost;
    }
    output.subtype = string_field(event, "subtype");
    output.is_error = event
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    output.api_error_status = event.get("api_error_status").map(status_string);
    if output.api_error_status.as_deref() == Some("429") {
        output.rate_limited = true;
    }
    output.result_text = string_field(event, "result").or_else(|| {
        event.get("errors").and_then(Value::as_array).map(|errors| {
            errors
                .iter()
                .map(status_string)
                .collect::<Vec<_>>()
                .join("; ")
        })
    });
}

fn usage_has_values(usage: &TokenUsage) -> bool {
    usage.input != 0
        || usage.output != 0
        || usage.reasoning != 0
        || usage.cache_read != 0
        || usage.cache_write != 0
        || usage.total != 0
        || usage.cost != 0.0
        || !usage.raw.is_empty()
}

fn parse_child(event: &Value, children: &mut Vec<ChildActivity>) {
    let subtype = string_field(event, "subtype");
    if !matches!(
        subtype.as_deref(),
        Some("task_started" | "task_progress" | "task_notification")
    ) {
        return;
    }
    let id = string_field(event, "task_id")
        .or_else(|| string_field(event, "uuid"))
        .or_else(|| subtype.clone())
        .unwrap_or_default();
    let existing = children.iter().find(|child| child.id == id).cloned();
    let usage = event.get("usage").and_then(Value::as_object);
    upsert(
        children,
        ChildActivity {
            id,
            label: string_field(event, "description")
                .or_else(|| string_field(event, "subagent_type"))
                .or_else(|| existing.as_ref().map(|item| item.label.clone()))
                .or(subtype.clone())
                .unwrap_or_default()
                .chars()
                .take(80)
                .collect(),
            status: if subtype.as_deref() == Some("task_notification") {
                "completed".into()
            } else {
                existing
                    .as_ref()
                    .map(|item| item.status.clone())
                    .unwrap_or_else(|| "running".into())
            },
            kind: string_field(event, "task_type")
                .or_else(|| string_field(event, "workflow_name"))
                .or_else(|| existing.as_ref().map(|item| item.kind.clone()))
                .unwrap_or_default(),
            last_tool: string_field(event, "last_tool_name")
                .or_else(|| existing.as_ref().and_then(|item| item.last_tool.clone())),
            tokens: usage
                .and_then(|item| {
                    item.get("total_tokens")
                        .or_else(|| item.get("totalTokens"))
                        .or_else(|| item.get("total"))
                })
                .and_then(u64_value)
                .or_else(|| existing.as_ref().map(|item| item.tokens))
                .unwrap_or(0),
        },
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn closes_native_children_on_success() {
        let output = parse(&stream(&[
            json!({"type":"system","subtype":"task_started","task_id":"a1","description":"Explore"}),
            json!({"type":"result","subtype":"success","session_id":"s1","result":"done"}),
        ]));
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(output.children[0].status, "completed");
    }

    #[test]
    fn aggregates_assistant_usage_and_normalizes_rate_limit_metadata() {
        let output = parse(&stream(&[
            json!({"type":"assistant","message":{"usage":{"input_tokens":10,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":4}}}),
            json!({"type":"assistant","message":{"usage":{"input_tokens":20,"output_tokens":5,"reasoning_output_tokens":2,"cache_read_input_tokens":6,"cache_creation_input_tokens":7}}}),
            json!({"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resets_at":2000,"rate_limit_type":"weekly"}}),
        ]));
        assert_eq!(output.usage.input, 30);
        assert_eq!(output.usage.output, 7);
        assert_eq!(output.usage.reasoning, 2);
        assert_eq!(output.usage.cache_read, 9);
        assert_eq!(output.usage.cache_write, 11);
        assert!(output.rate_limited);
        assert_eq!(output.resets_at, Some(2000.0));
        assert_eq!(output.limit_window.as_deref(), Some("weekly"));
    }

    #[test]
    fn uses_result_usage_as_the_authoritative_session_total() {
        let output = parse(include_bytes!(
            "../../../tests/fixtures/provider_events/claude-usage-result.jsonl"
        ));
        assert_eq!(output.usage.input, 30);
        assert_eq!(output.usage.output, 7);
        assert_eq!(output.usage.cache_read, 9);
        assert_eq!(output.usage.total, 57);
        assert_eq!(output.usage.cost, 0.42);
    }
}
