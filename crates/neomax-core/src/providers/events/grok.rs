use serde_json::Value;

use crate::providers::{ChildActivity, ParsedEvents, TokenUsage};

use super::common::{
    LIMIT_RE, close_running, epoch_now, json_lines, number_value, reset_epoch, string_field, upsert,
};

pub fn parse(bytes: &[u8]) -> ParsedEvents {
    parse_at(bytes, epoch_now())
}

pub fn parse_at(bytes: &[u8], now: f64) -> ParsedEvents {
    let mut output = ParsedEvents::default();
    let mut text = String::new();
    let mut ended = false;
    for event in json_lines(bytes) {
        match string_field(&event, "type").as_deref() {
            Some("text") => append_text(&event, &mut text),
            Some("tool_call") => parse_tool(&event, &mut output.children),
            Some("tool_call_update") => update_tool(&event, &mut output.children),
            Some("usage") => merge_usage(&event, false, &mut output),
            Some("error") => {
                merge_usage(&event, true, &mut output);
                parse_error(&event, now, &mut output);
            }
            Some("end") => {
                ended = true;
                parse_end(&event, now, &mut output);
            }
            _ => {}
        }
    }
    output.result_text = (!text.trim().is_empty()).then(|| text.trim().to_string());
    classify(&mut output, ended);
    output
}

fn append_text(event: &Value, text: &mut String) {
    if let Some(value) = event.get("data").and_then(Value::as_str) {
        text.push_str(value);
    }
}

fn parse_tool(event: &Value, children: &mut Vec<ChildActivity>) {
    let id =
        string_field(event, "toolCallId").unwrap_or_else(|| format!("tool-{}", children.len()));
    let name = string_field(event, "toolName")
        .or_else(|| string_field(event, "title"))
        .unwrap_or_else(|| "tool".into());
    let normalized = name.to_ascii_lowercase();
    upsert(
        children,
        ChildActivity {
            id,
            kind: if matches!(
                normalized.as_str(),
                "task" | "agent" | "subagent" | "spawn_subagent"
            ) {
                "agent".into()
            } else {
                "step".into()
            },
            label: string_field(event, "title")
                .unwrap_or(name)
                .chars()
                .take(80)
                .collect(),
            status: "running".into(),
            last_tool: None,
            tokens: 0,
        },
    );
}

fn update_tool(event: &Value, children: &mut [ChildActivity]) {
    let Some(id) = string_field(event, "toolCallId") else {
        return;
    };
    let Some(child) = children.iter_mut().find(|child| child.id == id) else {
        return;
    };
    match string_field(event, "status")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "failed" | "error" => child.status = "error".into(),
        "completed" | "success" | "ok" => child.status = "completed".into(),
        _ => {}
    }
}

fn merge_usage(event: &Value, authoritative: bool, output: &mut ParsedEvents) {
    if let Some(usage) = event.get("usage") {
        let usage = TokenUsage::from_grok_value(usage);
        if authoritative {
            if usage_has_values(&usage) {
                output.usage = usage;
            }
        } else {
            output.usage.add(&usage);
        }
    }
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

fn parse_error(event: &Value, now: f64, output: &mut ParsedEvents) {
    let error = event.get("error").unwrap_or(&Value::Null);
    let data = error.get("data").unwrap_or(&Value::Null);
    let message = string_field(event, "message")
        .or_else(|| string_field(error, "message"))
        .or_else(|| string_field(data, "message"))
        .unwrap_or_else(|| "Grok error".into());
    let body = string_field(data, "responseBody")
        .or_else(|| string_field(data, "response_body"))
        .unwrap_or_default();
    if LIMIT_RE.is_match(&format!("{message} {body}")) {
        output.rate_limited = true;
        output.api_error_status = Some("429".into());
        output.limit_window = Some("provider".into());
    }
    let status = event
        .get("status")
        .or_else(|| event.get("statusCode"))
        .or_else(|| event.get("status_code"))
        .or_else(|| error.get("status"))
        .or_else(|| error.get("statusCode"))
        .or_else(|| error.get("status_code"))
        .or_else(|| data.get("status"))
        .or_else(|| data.get("statusCode"))
        .or_else(|| data.get("status_code"))
        .map(super::common::status_string);
    if status.as_deref() == Some("429") {
        output.rate_limited = true;
        output.api_error_status = status;
        output.resets_at = event
            .get("responseHeaders")
            .or_else(|| event.get("response_headers"))
            .or_else(|| event.get("headers"))
            .or_else(|| error.get("responseHeaders"))
            .or_else(|| error.get("response_headers"))
            .or_else(|| error.get("headers"))
            .or_else(|| data.get("responseHeaders"))
            .or_else(|| data.get("response_headers"))
            .or_else(|| data.get("headers"))
            .and_then(|headers| reset_epoch(headers, now));
    }
    output.errors.push(message);
    if !body.is_empty() {
        output.errors.push(body);
    }
}

fn parse_end(event: &Value, now: f64, output: &mut ParsedEvents) {
    output.session_id = string_field(event, "sessionId").or(output.session_id.take());
    merge_usage(event, true, output);
    match string_field(event, "stopReason")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rate_limit" | "rate_limited" => {
            output.rate_limited = true;
            output.api_error_status = Some("429".into());
            output.limit_window = Some("provider".into());
            output.resets_at = event
                .get("responseHeaders")
                .or_else(|| event.get("response_headers"))
                .or_else(|| event.get("headers"))
                .and_then(|headers| reset_epoch(headers, now))
                .or_else(|| {
                    event
                        .get("resetsAt")
                        .or_else(|| event.get("resets_at"))
                        .and_then(number_value)
                });
        }
        "error" | "cancelled" | "refusal" => {
            output.is_error = true;
            output.subtype = Some("error_during_execution".into());
        }
        _ => {}
    }
}

fn classify(output: &mut ParsedEvents, ended: bool) {
    if LIMIT_RE.is_match(&output.errors.join(" ")) {
        output.rate_limited = true;
        output.limit_window = Some("provider".into());
    }
    if ended && !output.is_error && !output.rate_limited {
        output.subtype = Some("success".into());
        close_running(&mut output.children, "completed");
    } else if output.subtype.is_none() {
        output.is_error = true;
        output.subtype = Some(if output.errors.is_empty() {
            "incomplete".into()
        } else {
            "error_during_execution".into()
        });
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn captures_success_usage_and_native_agent() {
        let output = parse(&stream(&[
            json!({"type":"tool_call","toolCallId":"call_1","toolName":"spawn_subagent","title":"Explore"}),
            json!({"type":"tool_call_update","toolCallId":"call_1","status":"completed"}),
            json!({"type":"text","data":"GROK_OK"}),
            json!({"type":"end","stopReason":"end_turn","sessionId":"session_grok","usage":{"input_tokens":90,"output_tokens":20}}),
        ]));
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(output.usage.output, 20);
        assert_eq!(output.children[0].kind, "agent");
        assert_eq!(output.children[0].status, "completed");
    }

    #[test]
    fn classifies_provider_limits() {
        let output = parse(&stream(&[
            json!({"type":"error","message":"429 rate limit exceeded"}),
        ]));
        assert!(output.rate_limited);
        assert_eq!(output.api_error_status.as_deref(), Some("429"));
    }

    #[test]
    fn aggregates_turn_usage_but_deduplicates_terminal_echo() {
        let output = parse_at(
            &stream(&[
                json!({"type":"usage","usage":{"inputTokens":100,"outputTokens":20,"cachedReadTokens":10,"cacheCreationTokens":5,"reasoningTokens":3}}),
                json!({"type":"usage","usage":{"inputTokens":80,"outputTokens":7,"cachedReadTokens":8,"cacheCreationTokens":2,"reasoningTokens":1}}),
                json!({"type":"end","stopReason":"end_turn","usage":{"inputTokens":80,"outputTokens":7,"cachedReadTokens":8,"cacheCreationTokens":2,"reasoningTokens":1}}),
            ]),
            1_000.0,
        );
        assert_eq!(output.usage.input, 80);
        assert_eq!(output.usage.output, 7);
        assert_eq!(output.usage.reasoning, 1);
        assert_eq!(output.usage.cache_read, 8);
        assert_eq!(output.usage.cache_write, 2);
    }

    #[test]
    fn keeps_uncached_input_and_uses_final_usage_as_authoritative() {
        let output = parse_at(
            include_bytes!("../../../tests/fixtures/provider_events/grok-usage-end.jsonl"),
            1_000.0,
        );
        assert_eq!(output.usage.input, 100);
        assert_eq!(output.usage.output, 8);
        assert_eq!(output.usage.reasoning, 4);
        assert_eq!(output.usage.cache_read, 20);
        assert_eq!(output.usage.cache_write, 5);
        assert_eq!(output.usage.total, 133);
    }

    #[test]
    fn uses_retry_after_for_structured_grok_limit() {
        let output = parse_at(
            &stream(&[json!({
                "type":"error",
                "message":"request rejected",
                "status":429,
                "headers":{"retry-after":"120"}
            })]),
            1_000.0,
        );
        assert!(output.rate_limited);
        assert_eq!(output.resets_at, Some(1_120.0));
    }
}
