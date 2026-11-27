//! Antigravity CLI (`agy --output-format stream-json`) events.
//!
//! Every line carries an `event` name (`init`, `step_update` or `result`) and
//! a payload under the same key. The `result` payload is the `json` output
//! envelope, so a single `--output-format json` line parses the same way.

use serde_json::Value;

use crate::providers::{ChildActivity, ParsedEvents, TokenUsage};

use super::common::{
    LIMIT_RE, close_running, epoch_now, json_lines, string_field, u64_value, upsert,
};

pub fn parse(bytes: &[u8]) -> ParsedEvents {
    parse_at(bytes, epoch_now())
}

pub fn parse_at(bytes: &[u8], _now: f64) -> ParsedEvents {
    let mut output = ParsedEvents::default();
    let mut text = String::new();
    let mut response = None;
    let mut status = None;
    for line in json_lines(bytes) {
        let name = string_field(&line, "event");
        let payload = name
            .as_deref()
            .and_then(|name| line.get(name))
            .unwrap_or(&line);
        if let Some(id) = conversation_id(&line).or_else(|| conversation_id(payload)) {
            output.session_id = Some(id);
        }
        match name.as_deref() {
            Some("step_update") => parse_step(payload, &mut text, &mut output),
            Some("result") => {
                response = string_field(payload, "response").filter(|text| !text.trim().is_empty());
                status = string_field(payload, "status").map(|value| value.to_ascii_uppercase());
                if let Some(usage) = payload.get("usage") {
                    output.usage = usage_from_value(usage);
                }
                if let Some(error) = string_field(payload, "error").filter(|text| !text.is_empty())
                {
                    output.errors.push(error);
                }
            }
            None if line.get("status").is_some() && line.get("response").is_some() => {
                response = string_field(&line, "response").filter(|text| !text.trim().is_empty());
                status = string_field(&line, "status").map(|value| value.to_ascii_uppercase());
                if let Some(usage) = line.get("usage") {
                    output.usage = usage_from_value(usage);
                }
                if let Some(error) = string_field(&line, "error").filter(|text| !text.is_empty()) {
                    output.errors.push(error);
                }
            }
            _ => {}
        }
    }
    output.result_text = response.or_else(|| (!text.trim().is_empty()).then(|| text.trim().into()));
    classify(&mut output, status.as_deref());
    output
}

fn conversation_id(value: &Value) -> Option<String> {
    string_field(value, "conversation_id").filter(|id| !id.trim().is_empty())
}

fn parse_step(step: &Value, text: &mut String, output: &mut ParsedEvents) {
    let state = string_field(step, "state").unwrap_or_default();
    let done = state.eq_ignore_ascii_case("DONE");
    match string_field(step, "step_type").as_deref() {
        Some("agent_response") => {
            if let Some(delta) = step.get("text_delta").and_then(Value::as_str) {
                text.push_str(delta);
            }
        }
        Some("tool") => parse_tool(step, done, &mut output.children),
        _ => {}
    }
    if let Some(subagents) = step
        .get("subagent_info")
        .and_then(|info| info.get("subagents"))
        .and_then(Value::as_array)
    {
        for subagent in subagents {
            let Some(id) = conversation_id(subagent) else {
                continue;
            };
            let label = string_field(subagent, "role")
                .or_else(|| string_field(subagent, "type_name"))
                .unwrap_or_else(|| "subagent".into());
            upsert(
                &mut output.children,
                ChildActivity {
                    id,
                    kind: "agent".into(),
                    label: label.chars().take(80).collect(),
                    status: if done { "completed" } else { "running" }.into(),
                    last_tool: None,
                    tokens: 0,
                },
            );
        }
    }
}

fn parse_tool(step: &Value, done: bool, children: &mut Vec<ChildActivity>) {
    let info = step.get("tool_info").unwrap_or(&Value::Null);
    let name = string_field(step, "tool_name")
        .or_else(|| string_field(info, "name"))
        .unwrap_or_else(|| "tool".into());
    let id = step
        .get("step_index")
        .and_then(u64_value)
        .map_or_else(|| format!("tool-{}", children.len()), |index| format!("step-{index}"));
    let failed = info.get("error").is_some_and(|error| !error.is_null());
    let status = if failed {
        "error"
    } else if done {
        "completed"
    } else {
        "running"
    };
    let kind = if matches!(
        name.to_ascii_lowercase().as_str(),
        "invoke_subagent" | "define_subagent" | "browser_subagent"
    ) {
        "agent"
    } else {
        "step"
    };
    upsert(
        children,
        ChildActivity {
            id,
            kind: kind.into(),
            label: name.chars().take(80).collect(),
            status: status.into(),
            last_tool: Some(name),
            tokens: 0,
        },
    );
}

fn usage_from_value(value: &Value) -> TokenUsage {
    let mut usage = TokenUsage::from_value(value);
    if let Some(object) = value.as_object() {
        if let Some(thinking) = object.get("thinking_tokens").and_then(u64_value) {
            usage.reasoning = thinking;
        }
        if let Some(cached) = object.get("cache_read_tokens").and_then(u64_value) {
            usage.cache_read = cached;
        }
    }
    usage
}

fn classify(output: &mut ParsedEvents, status: Option<&str>) {
    if LIMIT_RE.is_match(&output.errors.join(" ")) {
        output.rate_limited = true;
        output.api_error_status = Some("429".into());
        output.limit_window = Some("provider".into());
    }
    match status {
        Some("SUCCESS") if !output.rate_limited => {
            output.subtype = Some("success".into());
            close_running(&mut output.children, "completed");
        }
        Some(_) | None => {
            output.is_error = true;
            output.subtype = Some(match status {
                None => "incomplete".into(),
                Some("CANCELED" | "INTERRUPTED") => "interrupted".into(),
                Some(_) => "error_during_execution".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::common::stream;
    use super::*;

    #[test]
    fn reads_conversation_response_usage_and_native_subagents() {
        let output = parse(&stream(&[
            json!({"event":"init","conversation_id":"c-1","init":{"cwd":"/w","tools":[],"permission_mode":"always-proceed"}}),
            json!({"event":"step_update","step_update":{"conversation_id":"c-1","step_index":1,"state":"DONE","step_type":"tool","tool_name":"invoke_subagent","tool_info":{"name":"invoke_subagent","parameters":{},"output":"ok"},"subagent_info":{"subagents":[{"type_name":"general","role":"Explore","conversation_id":"c-2"}]}}}),
            json!({"event":"step_update","step_update":{"conversation_id":"c-1","step_index":2,"state":"DONE","step_type":"agent_response","text_delta":"partial"}}),
            json!({"event":"result","result":{"conversation_id":"c-1","status":"SUCCESS","response":"NEON_OK","num_turns":1,"usage":{"input_tokens":90,"output_tokens":20,"thinking_tokens":7,"cache_read_tokens":30,"total_tokens":147}}}),
        ]));
        assert_eq!(output.session_id.as_deref(), Some("c-1"));
        assert_eq!(output.result_text.as_deref(), Some("NEON_OK"));
        assert_eq!(output.subtype.as_deref(), Some("success"));
        assert_eq!(
            (output.usage.input, output.usage.output, output.usage.reasoning, output.usage.cache_read),
            (90, 20, 7, 30)
        );
        assert!(output.children.iter().any(|child| child.id == "c-2" && child.kind == "agent"));
    }

    #[test]
    fn error_envelope_is_an_error_and_quota_text_is_a_limit() {
        let output = parse(br#"{"conversation_id":"","status":"ERROR","response":"","error":"RESOURCE_EXHAUSTED: quota exceeded","num_turns":0}"#);
        assert!(output.is_error);
        assert!(output.rate_limited);
        assert_eq!(output.session_id, None);
    }

    #[test]
    fn stream_without_result_is_incomplete() {
        let output = parse(&stream(&[
            json!({"event":"init","conversation_id":"c-9","init":{}}),
        ]));
        assert!(output.is_error);
        assert_eq!(output.subtype.as_deref(), Some("incomplete"));
        assert_eq!(output.session_id.as_deref(), Some("c-9"));
    }
}
