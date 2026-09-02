use serde_json::{Map, Value};

use crate::providers::{ChildActivity, ParsedEvents};

use super::codex_quota::CodexQuotaRefreshResult;
use super::codex_usage::CodexUsageAccumulator;
use super::common::{
    LIMIT_RE, close_running, epoch_now, json_lines, number_value, reset_epoch, status_string,
    string_field, upsert,
};

pub fn parse(bytes: &[u8]) -> ParsedEvents {
    parse_at(bytes, epoch_now())
}

pub fn parse_at(bytes: &[u8], now: f64) -> ParsedEvents {
    let mut output = ParsedEvents::default();
    let mut usage = CodexUsageAccumulator::default();
    let mut completed = false;
    let mut failed = false;
    for event in json_lines(bytes) {
        usage.observe(&event);
        if event.get("rate_limits").is_some()
            || event.get("rateLimits").is_some()
            || event
                .get("payload")
                .and_then(|payload| payload.get("rate_limits"))
                .is_some()
            || event
                .get("payload")
                .and_then(|payload| payload.get("rateLimits"))
                .is_some()
        {
            parse_rate_limits(&event, now, &mut output);
        }
        match string_field(&event, "type")
            .or_else(|| string_field(&event, "method"))
            .as_deref()
        {
            Some("thread.started") => {
                output.session_id = string_field(&event, "thread_id").or_else(|| {
                    event
                        .get("thread")
                        .and_then(|thread| string_field(thread, "id"))
                });
            }
            Some("thread/started") => {
                output.session_id = string_field(&event, "threadId")
                    .or_else(|| string_field(&event, "thread_id"))
                    .or_else(|| {
                        event
                            .get("params")
                            .and_then(|params| string_field(params, "threadId"))
                    });
            }
            Some(kind @ ("item.started" | "item.completed" | "item.updated")) => {
                parse_item(kind, &event, &mut output);
            }
            Some("item/started") => parse_item("item.started", &event, &mut output),
            Some("item/completed") => parse_item("item.completed", &event, &mut output),
            Some("item/updated") => parse_item("item.updated", &event, &mut output),
            Some("turn.completed") => {
                completed = true;
                parse_turn_completed(&event, now, &mut failed, &mut output);
            }
            Some("turn/completed") => {
                completed = true;
                parse_turn_completed(&event, now, &mut failed, &mut output);
            }
            Some("turn.failed") => {
                failed = true;
                let message = event
                    .get("error")
                    .and_then(Value::as_object)
                    .and_then(|error| error.get("message"))
                    .map(status_string)
                    .unwrap_or_else(|| status_string(event.get("error").unwrap_or(&Value::Null)));
                output.errors.push(message);
                parse_error_metadata(&event, now, &mut output);
            }
            Some("turn/failed") => {
                failed = true;
                let payload = event.get("params").unwrap_or(&event);
                let error = payload.get("error").unwrap_or(payload);
                let message = error
                    .get("message")
                    .map(status_string)
                    .unwrap_or_else(|| status_string(error));
                output.errors.push(message);
                parse_error_metadata(&event, now, &mut output);
            }
            Some("error") => {
                let payload = event.get("params").unwrap_or(&event);
                let error = payload.get("error").unwrap_or(payload);
                output.errors.push(
                    string_field(payload, "message")
                        .or_else(|| string_field(error, "message"))
                        .unwrap_or_else(|| status_string(error)),
                );
                parse_error_metadata(&event, now, &mut output);
            }
            Some("account/rateLimits/updated") => parse_rate_limits(&event, now, &mut output),
            _ => {}
        }
    }
    output.usage = usage.finish();
    if completed || failed {
        close_running(
            &mut output.children,
            if failed { "error" } else { "completed" },
        );
    }
    classify_terminal(&mut output, completed, failed);
    output
}

fn parse_turn_completed(event: &Value, now: f64, failed: &mut bool, output: &mut ParsedEvents) {
    let turn = event
        .get("turn")
        .or_else(|| event.get("params").and_then(|params| params.get("turn")))
        .unwrap_or(event);
    let status = string_field(turn, "status");
    if matches!(status.as_deref(), Some("failed" | "error")) {
        *failed = true;
    }
    if let Some(error) = turn.get("error").or_else(|| event.get("error")) {
        let message = error
            .get("message")
            .map(status_string)
            .unwrap_or_else(|| status_string(error));
        if !message.is_empty() {
            output.errors.push(message);
        }
    }
    parse_error_metadata(turn, now, output);
}

fn parse_error_metadata(event: &Value, now: f64, output: &mut ParsedEvents) {
    let mut containers = vec![event];
    if let Some(error) = event.get("error") {
        containers.push(error);
        for key in ["data", "response", "cause"] {
            if let Some(container) = error.get(key) {
                containers.push(container);
            }
        }
    }
    if let Some(params) = event.get("params") {
        containers.push(params);
        if let Some(error) = params.get("error") {
            containers.push(error);
            for key in ["data", "response", "cause"] {
                if let Some(container) = error.get(key) {
                    containers.push(container);
                }
            }
        }
    }
    for container in &containers {
        let status = container
            .get("status")
            .or_else(|| container.get("status_code"))
            .or_else(|| container.get("statusCode"))
            .or_else(|| container.get("api_error_status"))
            .map(status_string);
        if status.as_deref() == Some("429") {
            output.rate_limited = true;
            output.api_error_status = status;
        }
        if let Some(result) = container
            .get("rate_limits")
            .or_else(|| container.get("rateLimits"))
            .and_then(|value| CodexQuotaRefreshResult::from_value(value.clone(), now))
        {
            result.apply_to(output);
            if result.blocks_new_work() {
                output.rate_limited = true;
                output.api_error_status = Some("429".into());
            }
        }
    }
    if output.rate_limited && output.resets_at.is_none() {
        let header_reset = containers
            .iter()
            .find_map(|container| {
                ["responseHeaders", "response_headers", "headers"]
                    .iter()
                    .find_map(|key| container.get(*key))
            })
            .and_then(|headers| reset_epoch(headers, now));
        let retry_after = containers.iter().find_map(|container| {
            ["retryAfter", "retry_after", "retry-after"]
                .iter()
                .find_map(|key| container.get(*key).and_then(number_value))
        });
        output.resets_at = header_reset.or_else(|| retry_after.map(|seconds| now + seconds));
    }
}

fn parse_rate_limits(event: &Value, now: f64, output: &mut ParsedEvents) {
    let value = event.get("params").unwrap_or(event);
    let Some(result) = CodexQuotaRefreshResult::from_value(value.clone(), now) else {
        return;
    };
    if result.blocks_new_work() {
        output.rate_limited = true;
        output.api_error_status = Some("429".into());
    }
    result.apply_to(output);
}

fn parse_item(kind: &str, event: &Value, output: &mut ParsedEvents) {
    let Some(item) = event
        .get("item")
        .or_else(|| event.get("params").and_then(|params| params.get("item")))
        .and_then(Value::as_object)
    else {
        return;
    };
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    if item_type == "agent_message" {
        if kind == "item.completed" {
            output.result_text = item
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or(output.result_text.take());
        }
        return;
    }
    if !is_child_type(item_type) {
        return;
    }
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("item-{}", output.children.len()));
    upsert(
        &mut output.children,
        ChildActivity {
            id,
            kind: if is_step_type(item_type) {
                "step".into()
            } else {
                "agent".into()
            },
            label: child_label(item, item_type),
            status: child_status(kind, item).into(),
            last_tool: None,
            tokens: 0,
        },
    );
}

fn classify_terminal(output: &mut ParsedEvents, completed: bool, failed: bool) {
    let blob = format!(
        "{} {}",
        output.errors.join(" "),
        output.result_text.as_deref().unwrap_or_default()
    );
    output.rate_limited |= LIMIT_RE.is_match(&blob);
    if completed && !failed && !output.rate_limited {
        output.subtype = Some("success".into());
        return;
    }
    output.is_error = true;
    output.subtype = Some(if failed {
        "error_during_execution".into()
    } else {
        "incomplete".into()
    });
}

fn is_child_type(item_type: &str) -> bool {
    matches!(
        item_type,
        "command_execution"
            | "tool_call"
            | "mcp_tool_call"
            | "collab_agent"
            | "subagent"
            | "agent_run"
            | "task"
    )
}

fn is_step_type(item_type: &str) -> bool {
    matches!(
        item_type,
        "command_execution" | "tool_call" | "mcp_tool_call"
    )
}

fn child_status<'a>(event_kind: &str, item: &'a Map<String, Value>) -> &'a str {
    if event_kind == "item.started" {
        return "running";
    }
    match item.get("status").and_then(Value::as_str) {
        Some("failed" | "error") => "error",
        Some("completed" | "success" | "ok") => "completed",
        _ if item.contains_key("exit_code") => {
            if item
                .get("exit_code")
                .is_none_or(|value| value.is_null() || value.as_i64() == Some(0))
            {
                "completed"
            } else {
                "error"
            }
        }
        _ => "completed",
    }
}

fn child_label(item: &Map<String, Value>, item_type: &str) -> String {
    ["command", "name"]
        .iter()
        .find_map(|key| item.get(*key).and_then(Value::as_str))
        .unwrap_or(item_type)
        .chars()
        .take(80)
        .collect()
}

#[cfg(test)]
mod tests;
