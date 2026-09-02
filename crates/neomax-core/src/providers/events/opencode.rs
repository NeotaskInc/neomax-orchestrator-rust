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
    let mut stopped = false;
    for event in json_lines(bytes) {
        if output.session_id.is_none() {
            output.session_id =
                string_field(&event, "sessionID").or_else(|| string_field(&event, "session_id"));
        }
        let part = event.get("part").and_then(Value::as_object);
        match string_field(&event, "type").as_deref() {
            Some("text") => parse_text(&event, part, &mut output),
            Some("tool_use") => parse_tool(part, &mut output.children),
            Some("step_finish") => {
                stopped |= parse_finish(&event, part, &mut output.usage);
            }
            Some("error") => parse_error(&event, now, &mut output),
            _ => {}
        }
    }
    if stopped && !output.rate_limited && output.errors.is_empty() {
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
    output
}

fn parse_text(event: &Value, part: Option<&Map<String, Value>>, output: &mut ParsedEvents) {
    output.result_text = part
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| string_field(event, "text"));
}

fn parse_tool(part: Option<&Map<String, Value>>, children: &mut Vec<ChildActivity>) {
    let Some(part) = part else { return };
    let state = part.get("state").and_then(Value::as_object);
    let id = part
        .get("callID")
        .or_else(|| part.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("tool-{}", children.len()));
    let tool = part.get("tool").and_then(Value::as_str).unwrap_or("tool");
    let status = match state
        .and_then(|item| item.get("status"))
        .and_then(Value::as_str)
    {
        Some("completed" | "success" | "ok") => "completed",
        Some("failed" | "error") => "error",
        Some(value) => value,
        None => "running",
    };
    upsert(
        children,
        ChildActivity {
            id,
            kind: if matches!(tool, "task" | "general" | "explore" | "scout") {
                "agent".into()
            } else {
                "step".into()
            },
            label: state
                .and_then(|item| item.get("title"))
                .and_then(Value::as_str)
                .unwrap_or(tool)
                .chars()
                .take(80)
                .collect(),
            status: status.into(),
            last_tool: None,
            tokens: 0,
        },
    );
}

fn parse_finish(event: &Value, part: Option<&Map<String, Value>>, usage: &mut TokenUsage) -> bool {
    let reason = part
        .and_then(|item| item.get("reason"))
        .and_then(Value::as_str)
        .or_else(|| event.get("reason").and_then(Value::as_str));
    let tokens = part
        .and_then(|item| item.get("tokens"))
        .filter(|value| value.is_object())
        .or_else(|| part.and_then(|item| item.get("usage")))
        .filter(|value| value.is_object())
        .or_else(|| event.get("tokens"))
        .filter(|value| value.is_object())
        .or_else(|| event.get("usage"));
    usage.add(&TokenUsage::from_value(tokens.unwrap_or(&Value::Null)));
    usage.cost += part
        .and_then(|item| item.get("cost"))
        .and_then(number_value)
        .or_else(|| event.get("cost").and_then(number_value))
        .unwrap_or(0.0);
    reason == Some("stop")
}

fn parse_error(event: &Value, now: f64, output: &mut ParsedEvents) {
    let error = event.get("error").unwrap_or(&Value::Null);
    let data = error.get("data").and_then(Value::as_object);
    let message = data
        .and_then(|item| item.get("message"))
        .or_else(|| error.get("message"))
        .or_else(|| event.get("message"))
        .map(status_string)
        .unwrap_or_else(|| status_string(error));
    let body = data
        .and_then(|item| item.get("responseBody"))
        .map(status_string)
        .unwrap_or_default();
    let status = data
        .and_then(|item| {
            item.get("statusCode")
                .or_else(|| item.get("status_code"))
                .or_else(|| item.get("status"))
        })
        .or_else(|| {
            error
                .get("statusCode")
                .or_else(|| error.get("status_code"))
                .or_else(|| error.get("status"))
        })
        .or_else(|| {
            event
                .get("statusCode")
                .or_else(|| event.get("status_code"))
                .or_else(|| event.get("status"))
        })
        .map(status_string);
    let headers = data
        .and_then(|item| {
            item.get("responseHeaders")
                .or_else(|| item.get("response_headers"))
                .or_else(|| item.get("headers"))
        })
        .or_else(|| {
            error
                .get("responseHeaders")
                .or_else(|| error.get("response_headers"))
                .or_else(|| error.get("headers"))
        })
        .or_else(|| {
            event
                .get("responseHeaders")
                .or_else(|| event.get("response_headers"))
                .or_else(|| event.get("headers"))
        });
    output.errors.push(message.clone());
    if !body.is_empty() {
        output.errors.push(body.clone());
    }
    output.api_error_status = status.clone();
    if status.as_deref() == Some("429") || LIMIT_RE.is_match(&format!("{message} {body}")) {
        output.rate_limited = true;
        output.resets_at = headers
            .and_then(|headers| reset_epoch(headers, now))
            .or_else(|| {
                ["retryAfter", "retry_after", "retry-after"]
                    .iter()
                    .find_map(|key| event.get(*key).and_then(number_value))
                    .map(|seconds| now + seconds)
            });
        output.limit_window = Some("provider".into());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn captures_success_usage_children_and_retry_window() {
        let output = parse_at(
            &stream(&[
                json!({"type":"step_start","sessionID":"ses_ox"}),
                json!({"type":"tool_use","part":{"tool":"task","callID":"call_1","state":{"status":"running","title":"Explore"}}}),
                json!({"type":"text","part":{"text":"done"}}),
                json!({"type":"step_finish","part":{"reason":"stop","tokens":{"total":30,"input":20,"output":7,"reasoning":3,"cache":{"read":8}}}}),
            ]),
            1_000.0,
        );
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(output.session_id.as_deref(), Some("ses_ox"));
        assert_eq!(output.usage.total, 30);
        assert_eq!(output.usage.cache_read, 8);
        assert_eq!(output.children[0].kind, "agent");

        let limited = parse_at(
            &stream(&[
                json!({"type":"error","error":{"data":{"message":"Too many requests","statusCode":"429","responseHeaders":{"retry-after":"90"}}}}),
            ]),
            1_000.0,
        );
        assert!(limited.rate_limited);
        assert_eq!(limited.resets_at, Some(1_090.0));
    }

    #[test]
    fn accumulates_step_usage_without_dropping_token_dimensions() {
        let output = parse_at(
            include_bytes!("../../../tests/fixtures/provider_events/opencode-steps.jsonl"),
            1_000.0,
        );
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(output.usage.total, 32);
        assert_eq!(output.usage.input, 18);
        assert_eq!(output.usage.output, 6);
        assert_eq!(output.usage.reasoning, 5);
        assert_eq!(output.usage.cache_read, 8);
        assert_eq!(output.usage.cache_write, 3);
    }

    #[test]
    fn does_not_report_success_when_a_late_rate_limit_follows_stop() {
        let output = parse_at(
            &stream(&[
                json!({"type":"step_finish","part":{"reason":"stop","tokens":{"total":3}}}),
                json!({"type":"error","error":{"data":{"message":"rate limit","statusCode":429}}}),
            ]),
            1_000.0,
        );
        assert!(output.rate_limited);
        assert!(output.is_error);
        assert_eq!(output.subtype.as_deref(), Some("error_during_execution"));
    }

    #[test]
    fn records_top_level_rate_limit_headers() {
        let output = parse_at(
            &stream(&[json!({
                "type": "error",
                "status": 429,
                "headers": {"retry-after": "90"},
                "message": "request throttled"
            })]),
            1_000.0,
        );
        assert!(output.rate_limited);
        assert_eq!(output.resets_at, Some(1_090.0));
    }
}
