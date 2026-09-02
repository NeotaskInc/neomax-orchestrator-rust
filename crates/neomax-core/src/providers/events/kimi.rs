use serde_json::{Map, Value};

use crate::providers::{ChildActivity, ParsedEvents, TokenUsage};

use super::common::{
    LIMIT_RE, close_running, epoch_now, json_lines, number_value, reset_epoch, status_string,
    string_field, upsert,
};

pub fn parse(bytes: &[u8]) -> ParsedEvents {
    parse_at(bytes, epoch_now())
}

pub fn parse_at(bytes: &[u8], now: f64) -> ParsedEvents {
    let mut output = ParsedEvents::default();
    let mut terminal = false;
    for event in json_lines(bytes) {
        let role = string_field(&event, "role");
        if role.as_deref() == Some("meta")
            && string_field(&event, "type").as_deref() == Some("session.resume_hint")
        {
            output.session_id = string_field(&event, "session_id").or(output.session_id);
            terminal = true;
            continue;
        }
        match role.as_deref() {
            Some("assistant") => parse_assistant(&event, &mut output),
            Some("tool") => resolve_tool(&event, &mut output.children),
            _ => {}
        }
        if let Some(usage) = event.get("usage").or_else(|| {
            event
                .get("message")
                .and_then(|message| message.get("usage"))
        }) {
            output.usage.add(&TokenUsage::from_value(usage));
        }
        if matches!(role.as_deref(), Some("error" | "system"))
            || string_field(&event, "type").as_deref() == Some("error")
        {
            parse_error(&event, now, &mut output);
        }
    }
    classify(&mut output, terminal);
    output
}

fn parse_assistant(event: &Value, output: &mut ParsedEvents) {
    if let Some(content) = content_text(event.get("content")) {
        if !content.trim().is_empty() {
            output.result_text = Some(content);
        }
    }
    if let Some(calls) = event.get("tool_calls").and_then(Value::as_array) {
        for call in calls.iter().filter_map(Value::as_object) {
            parse_tool(call, &mut output.children);
        }
    }
}

fn parse_tool(call: &Map<String, Value>, children: &mut Vec<ChildActivity>) {
    let function = call.get("function").and_then(Value::as_object);
    let id = call
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("tool-{}", children.len()));
    let name = call
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| {
            function
                .and_then(|item| item.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or("tool");
    let normalized = name.to_ascii_lowercase();
    upsert(
        children,
        ChildActivity {
            id,
            kind: if matches!(
                normalized.as_str(),
                "task" | "agent" | "agentswarm" | "senddmail"
            ) {
                "agent".into()
            } else {
                "step".into()
            },
            label: name.chars().take(80).collect(),
            status: "running".into(),
            last_tool: None,
            tokens: 0,
        },
    );
}

fn resolve_tool(event: &Value, children: &mut [ChildActivity]) {
    let id = string_field(event, "tool_call_id").or_else(|| string_field(event, "id"));
    if let Some(child) = id
        .as_deref()
        .and_then(|id| children.iter_mut().find(|child| child.id == id))
    {
        child.status = if event.get("error").is_some_and(|value| !value.is_null()) {
            "error".into()
        } else {
            match string_field(event, "status")
                .or_else(|| string_field(event, "error"))
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "failed" | "error" => "error".into(),
                _ => "completed".into(),
            }
        };
    }
}

fn parse_error(event: &Value, now: f64, output: &mut ParsedEvents) {
    let error = event.get("error");
    let mut containers = event.as_object().into_iter().collect::<Vec<_>>();
    if let Some(error) = error.and_then(Value::as_object) {
        containers.push(error);
        for key in ["data", "response", "cause"] {
            if let Some(container) = error.get(key).and_then(Value::as_object) {
                containers.push(container);
            }
        }
    }
    let status = containers.iter().find_map(|item| {
        ["statusCode", "status_code", "status"]
            .iter()
            .find_map(|key| item.get(*key).map(status_string))
    });
    let headers = containers.iter().find_map(|item| {
        ["responseHeaders", "response_headers", "headers"]
            .iter()
            .find_map(|key| item.get(*key))
    });
    let message = event
        .get("message")
        .or_else(|| error.and_then(|value| value.get("message")))
        .or(error)
        .or_else(|| event.get("content"))
        .map(status_string);
    if let Some(message) = message {
        output.errors.push(message);
    }
    if status.as_deref() == Some("429") || LIMIT_RE.is_match(&message_string(event, error)) {
        output.api_error_status = status;
        output.rate_limited = true;
        output.resets_at = headers
            .and_then(|headers| reset_epoch(headers, now))
            .or_else(|| {
                containers
                    .iter()
                    .find_map(|container| {
                        ["retryAfter", "retry_after", "retry-after"]
                            .iter()
                            .find_map(|key| container.get(*key).and_then(number_value))
                    })
                    .map(|seconds| now + seconds)
            });
        output.limit_window = Some("provider".into());
    }
}

fn message_string(event: &Value, error: Option<&Value>) -> String {
    event
        .get("message")
        .or_else(|| error.and_then(|value| value.get("message")))
        .or(error)
        .or_else(|| event.get("content"))
        .map(status_string)
        .unwrap_or_default()
}

fn classify(output: &mut ParsedEvents, terminal: bool) {
    let blob = format!(
        "{} {}",
        output.errors.join(" "),
        output.result_text.as_deref().unwrap_or_default()
    );
    if LIMIT_RE.is_match(&blob) {
        output.rate_limited = true;
        output.limit_window = Some("provider".into());
    }
    if terminal && output.errors.is_empty() && !output.rate_limited {
        output.subtype = Some("success".into());
        close_running(&mut output.children, "completed");
    } else {
        output.is_error = true;
        output.subtype = Some(if output.errors.is_empty() && !output.rate_limited {
            "incomplete".into()
        } else {
            "error_during_execution".into()
        });
    }
}

fn content_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value.clone()),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|item| {
                    item.get("text")
                        .or_else(|| item.get("content"))
                        .map(status_string)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn captures_terminal_session_and_native_agent() {
        let output = parse_at(
            &stream(&[
                json!({"role":"assistant","content":"working","tool_calls":[{"id":"call_1","function":{"name":"Agent"}}]}),
                json!({"role":"tool","tool_call_id":"call_1"}),
                json!({"role":"assistant","content":"complete"}),
                json!({"role":"meta","type":"session.resume_hint","session_id":"session_kimi"}),
            ]),
            1_000.0,
        );
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(output.result_text.as_deref(), Some("complete"));
        assert_eq!(output.children[0].kind, "agent");
        assert_eq!(output.children[0].status, "completed");
    }

    #[test]
    fn records_structured_retry_after() {
        let output = parse_at(
            &stream(&[
                json!({"role":"error","error":{"message":"rate limit","statusCode":429,"responseHeaders":{"retry-after":"120"}}}),
            ]),
            1_000.0,
        );
        assert!(output.rate_limited);
        assert_eq!(output.resets_at, Some(1_120.0));
    }

    #[test]
    fn aggregates_usage_records_and_preserves_reasoning_and_cache_fields() {
        let output = parse_at(
            include_bytes!("../../../tests/fixtures/provider_events/kimi-usage.jsonl"),
            1_000.0,
        );
        assert_eq!(output.usage.input, 30);
        assert_eq!(output.usage.output, 7);
        assert_eq!(output.usage.reasoning, 3);
        assert_eq!(output.usage.cache_read, 9);
        assert_eq!(output.usage.cache_write, 11);
    }

    #[test]
    fn records_reset_from_top_level_retry_after() {
        let output = parse_at(
            &stream(&[json!({
                "type":"error",
                "status":429,
                "message":"rate limit",
                "retry_after":90
            })]),
            1_000.0,
        );
        assert!(output.rate_limited);
        assert_eq!(output.resets_at, Some(1_090.0));
    }
}
