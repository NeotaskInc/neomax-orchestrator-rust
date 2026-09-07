use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::headers::timestamp_epoch;

const PREVIEW_CHARS: usize = 4_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordedText {
    pub text: String,
    pub at: Option<i64>,
    pub truncated: bool,
}

impl RecordedText {
    fn new(text: &str, at: Option<i64>) -> Self {
        let mut chars = text.chars();
        let text = chars.by_ref().take(PREVIEW_CHARS).collect();
        Self {
            text,
            at,
            truncated: chars.next().is_some(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordedTool {
    pub name: String,
    pub id: Option<String>,
    pub input: RecordedText,
    pub result: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TranscriptActivity {
    pub task: Option<RecordedText>,
    pub last_message: Option<RecordedText>,
    pub last_tool: Option<RecordedTool>,
    pub updated_at: Option<i64>,
    pub model: Option<String>,
}

impl TranscriptActivity {
    pub(crate) fn observe_claude(&mut self, event: &Value) {
        let at = self.observe_time(event);
        let kind = event.get("type").and_then(Value::as_str);
        let Some(message) = event.get("message") else {
            return;
        };
        if let Some(model) = message
            .get("model")
            .and_then(Value::as_str)
            .filter(|v| !v.starts_with('<'))
        {
            self.model = Some(model.into());
        }
        let content = message.get("content").unwrap_or(&Value::Null);
        if kind == Some("user") && self.task.is_none() {
            if let Some(text) = content_text(content).filter(|text| !metadata_text(text)) {
                self.task = Some(RecordedText::new(&text, at));
            }
        }
        if kind == Some("assistant") {
            if let Some(text) = content_text(content) {
                self.last_message = Some(RecordedText::new(&text, at));
            }
        }
        for block in content.as_array().into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") if kind == Some("assistant") => self.tool(
                    block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Unnamed tool"),
                    block.get("id"),
                    block.get("input"),
                    at,
                ),
                Some("tool_result") => self.result(
                    block.get("tool_use_id"),
                    if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                        "error reported"
                    } else {
                        "result recorded"
                    },
                ),
                _ => {}
            }
        }
    }

    pub(crate) fn observe_codex(&mut self, event: &Value) {
        let at = self.observe_time(event);
        let payload = event.get("payload").unwrap_or(event);
        if let Some(model) = payload.get("model").and_then(Value::as_str) {
            self.model = Some(model.into());
        }
        let kind = payload.get("type").and_then(Value::as_str);
        match kind {
            Some("user_message") if self.task.is_none() => {
                if let Some(text) = payload
                    .get("message")
                    .and_then(Value::as_str)
                    .filter(|text| !metadata_text(text))
                {
                    self.task = Some(RecordedText::new(text, at));
                }
            }
            Some("agent_message") => {
                if let Some(text) = payload.get("message").and_then(Value::as_str) {
                    self.last_message = Some(RecordedText::new(text, at));
                }
            }
            Some("message") => {
                let role = payload.get("role").and_then(Value::as_str);
                if !matches!(role, Some("assistant" | "user")) {
                    return;
                }
                if let Some(text) = payload.get("content").and_then(content_text) {
                    if role == Some("assistant") {
                        self.last_message = Some(RecordedText::new(&text, at));
                    }
                    if role == Some("user") && self.task.is_none() && !metadata_text(&text) {
                        self.task = Some(RecordedText::new(&text, at));
                    }
                }
            }
            Some("function_call" | "custom_tool_call") => self.tool(
                payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Unnamed tool"),
                payload.get("call_id"),
                payload.get("arguments").or_else(|| payload.get("input")),
                at,
            ),
            Some("function_call_output" | "custom_tool_call_output") => {
                self.result(payload.get("call_id"), "result recorded")
            }
            _ => {}
        }
    }

    fn observe_time(&mut self, event: &Value) -> Option<i64> {
        let at = event.get("timestamp").and_then(timestamp_epoch);
        if let Some(at) = at {
            self.updated_at = Some(self.updated_at.map_or(at, |old| old.max(at)));
        }
        at
    }

    fn tool(&mut self, name: &str, id: Option<&Value>, input: Option<&Value>, at: Option<i64>) {
        let text = input
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            })
            .unwrap_or_default();
        self.last_tool = Some(RecordedTool {
            name: name.into(),
            id: id.and_then(Value::as_str).map(str::to_owned),
            input: RecordedText::new(&text, at),
            result: None,
        });
    }

    fn result(&mut self, id: Option<&Value>, result: &str) {
        if let Some(tool) = self.last_tool.as_mut() {
            if id
                .and_then(Value::as_str)
                .is_some_and(|id| tool.id.as_deref() == Some(id))
            {
                tool.result = Some(result.into());
            }
        }
    }
}

fn content_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str().filter(|text| !text.trim().is_empty()) {
        return Some(text.into());
    }
    let text = value
        .as_array()?
        .iter()
        .filter_map(|block| {
            matches!(
                block.get("type").and_then(Value::as_str),
                Some("text" | "input_text" | "output_text")
            )
            .then(|| block.get("text").and_then(Value::as_str))
            .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty()).then_some(text)
}

fn metadata_text(text: &str) -> bool {
    [
        "Caveat:",
        "<command-",
        "<local-command",
        "<system-reminder",
        "<task-notification",
        "# AGENTS.md instructions",
        "<environment_context>",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_codex_messages_custom_tools_and_result_ids_are_projected() {
        let mut activity = TranscriptActivity::default();
        for payload in [
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /fixture"}]}),
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"Fix the invoice retry"}]}),
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"I found the retry owner."}]}),
            json!({"type":"custom_tool_call","name":"apply_patch","call_id":"latest","input":"patch invoice.rs"}),
            json!({"type":"custom_tool_call_output","call_id":"other","output":"old result"}),
        ] {
            activity.observe_codex(&json!({"timestamp":"2026-09-07T09:00:00Z","type":"response_item","payload":payload}));
        }
        assert_eq!(
            activity.task.as_ref().unwrap().text,
            "Fix the invoice retry"
        );
        assert_eq!(
            activity.last_message.as_ref().unwrap().text,
            "I found the retry owner."
        );
        assert_eq!(activity.last_tool.as_ref().unwrap().name, "apply_patch");
        assert!(activity.last_tool.as_ref().unwrap().result.is_none());
        activity.observe_codex(
            &json!({"payload":{"type":"custom_tool_call_output","call_id":"latest"}}),
        );
        assert_eq!(
            activity.last_tool.as_ref().unwrap().result.as_deref(),
            Some("result recorded")
        );
        assert!(activity.updated_at.is_some());
    }

    #[test]
    fn claude_text_and_tools_survive_later_tool_only_messages() {
        let mut activity = TranscriptActivity::default();
        activity
            .observe_claude(&json!({"type":"user","message":{"content":"Review the migration"}}));
        activity.observe_claude(&json!({"type":"assistant","message":{"model":"claude-fixture","content":[{"type":"text","text":"Checking data retention."},{"type":"tool_use","id":"a","name":"Read","input":{"file_path":"migration.rs"}}]}}));
        activity.observe_claude(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a","is_error":true}]}}));
        assert_eq!(activity.task.unwrap().text, "Review the migration");
        assert_eq!(
            activity.last_message.unwrap().text,
            "Checking data retention."
        );
        assert_eq!(
            activity.last_tool.unwrap().result.as_deref(),
            Some("error reported")
        );
        assert_eq!(activity.model.as_deref(), Some("claude-fixture"));
    }

    #[test]
    fn display_preview_is_unicode_safe_and_explicitly_bounded() {
        let text = RecordedText::new(&"界".repeat(PREVIEW_CHARS + 10), None);
        assert_eq!(text.text.chars().count(), PREVIEW_CHARS);
        assert!(text.truncated);
    }
}
