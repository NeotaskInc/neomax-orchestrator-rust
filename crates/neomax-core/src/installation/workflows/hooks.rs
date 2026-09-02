use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::{Error, Result};

use super::super::files::{path_exists, read_bounded};
use super::manifest::{MAX_SETTINGS_BYTES, WorkflowManifest};
use super::support::shell_quote;

pub(super) type ValueMap = Map<String, Value>;
pub(super) type HookCommands = Vec<(String, String)>;

pub(super) fn read_settings(path: &Path) -> Result<Option<ValueMap>> {
    if !path_exists(path) {
        return Ok(None);
    }
    let bytes = read_bounded(path, MAX_SETTINGS_BYTES)?;
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|error| Error::InvalidState {
        path: path.to_path_buf(),
        message: format!("Claude settings JSON is invalid: {error}"),
    })?;
    value
        .as_object()
        .cloned()
        .map(Some)
        .ok_or_else(|| Error::InvalidState {
            path: path.to_path_buf(),
            message: "Claude settings must be a JSON object".into(),
        })
}

pub(super) fn merge_hooks(
    existing: Option<ValueMap>,
    bin: &Path,
) -> Result<(ValueMap, HookCommands)> {
    let mut settings = existing.unwrap_or_default();
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let object = hooks
        .as_object_mut()
        .ok_or_else(|| Error::Conflict("Claude settings hooks must be a JSON object".into()))?;
    let executable = shell_quote(bin)?;
    let commands = [
        ("SessionStart", format!("{executable} ls --hook"), 10),
        ("SessionStart", format!("{executable} orient --hook"), 10),
        ("Stop", format!("{executable} usage-hook"), 8),
        ("UserPromptSubmit", format!("{executable} turn-hook"), 8),
    ];
    let mut owned = Vec::new();
    for (event, command, timeout) in commands {
        if add_hook(object, event, &command, timeout)? {
            owned.push((event.into(), command));
        }
    }
    Ok((settings, owned))
}

fn add_hook(
    events: &mut Map<String, Value>,
    event: &str,
    command: &str,
    timeout: u64,
) -> Result<bool> {
    let groups = events
        .entry(event)
        .or_insert_with(|| Value::Array(Vec::new()));
    let groups = groups
        .as_array_mut()
        .ok_or_else(|| Error::Conflict(format!("Claude hook event {event} must be an array")))?;
    if let Some(group) = groups.iter_mut().find_map(|group| {
        let object = group.as_object_mut()?;
        (object.get("matcher").and_then(Value::as_str) == Some("")).then_some(object)
    }) {
        let hooks = group
            .entry("hooks")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(hooks) = hooks.as_array_mut() {
            if !hooks
                .iter()
                .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
            {
                hooks.push(
                    serde_json::json!({"type":"command","command":command,"timeout":timeout}),
                );
            }
        }
        return Ok(false);
    }
    groups.push(serde_json::json!({
        "matcher": "",
        "hooks": [{"type":"command","command":command,"timeout":timeout}]
    }));
    Ok(true)
}

pub(super) fn remove_hooks(settings: &mut ValueMap, owned: &BTreeSet<(String, String)>) {
    let Some(events) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let event_names = events.keys().cloned().collect::<Vec<_>>();
    for event in event_names {
        let Some(groups) = events.get_mut(&event).and_then(Value::as_array_mut) else {
            continue;
        };
        for group in groups.iter_mut() {
            let Some(object) = group.as_object_mut() else {
                continue;
            };
            if object.get("matcher").and_then(Value::as_str) != Some("") {
                continue;
            }
            if let Some(hooks) = object.get_mut("hooks").and_then(Value::as_array_mut) {
                hooks.retain(|hook| {
                    let Some(command) = hook.get("command").and_then(Value::as_str) else {
                        return true;
                    };
                    !owned.iter().any(|(owned_event, owned_command)| {
                        (owned_event.is_empty() || owned_event == &event)
                            && owned_command == command
                    })
                });
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|hooks| !hooks.is_empty())
        });
        if groups.is_empty() {
            events.remove(&event);
        }
    }
    if events.is_empty() {
        settings.remove("hooks");
    }
}

pub(super) fn write_json_stage(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

pub(super) fn write_json_stage_private(path: &Path, value: &impl Serialize) -> Result<()> {
    write_json_stage(path, value)?;
    crate::io::set_private_path(path)
}

pub(super) fn write_json_value_atomic(path: &Path, value: &ValueMap) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    crate::atomic::write_bytes_atomic(path, &bytes)
}

pub(super) fn unique_settings(manifest: &WorkflowManifest) -> BTreeSet<String> {
    manifest
        .hooks
        .iter()
        .map(|hook| hook.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use serde_json::Map;

    use super::*;

    #[test]
    fn shell_quote_keeps_metacharacters_inside_one_argument() {
        let quoted = shell_quote(Path::new("/tmp/neomax; touch /tmp/should-not-run")).unwrap();
        #[cfg(windows)]
        assert_eq!(
            quoted,
            "\"/tmp/neomax; touch /tmp/should-not-run\""
        );
        #[cfg(not(windows))]
        assert_eq!(quoted, "'/tmp/neomax; touch /tmp/should-not-run'");
    }

    #[cfg(unix)]
    #[test]
    fn shell_quote_rejects_non_utf8_hook_executables() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let value = OsString::from_vec(b"/tmp/neomax-\xff".to_vec());
        let path = Path::new(&value);
        let error = shell_quote(path).unwrap_err();
        assert!(error.to_string().contains("workflow hook executable path"));
    }

    #[test]
    fn hooks_use_an_empty_matcher_group_without_rewriting_narrow_groups() {
        let mut events = Map::new();
        events.insert(
            "SessionStart".into(),
            serde_json::json!([{
                "matcher": "project:*",
                "hooks": [{"type":"command","command":"user-command"}]
            }]),
        );
        assert!(add_hook(&mut events, "SessionStart", "neomax orient --hook", 10).unwrap());
        let groups = events["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0]["matcher"], "project:*");
        assert_eq!(groups[1]["matcher"], "");
        assert_eq!(groups[1]["hooks"][0]["command"], "neomax orient --hook");
    }

    #[test]
    fn hook_removal_keeps_a_user_command_in_a_narrow_matcher_group() {
        let mut settings = Map::new();
        settings.insert(
            "hooks".into(),
            serde_json::json!({
                "SessionStart": [
                    {"matcher":"project:*","hooks":[{"type":"command","command":"neomax orient --hook"}]},
                    {"matcher":"","hooks":[{"type":"command","command":"neomax orient --hook"}]}
                ]
            }),
        );
        let owned = BTreeSet::from([("SessionStart".into(), "neomax orient --hook".into())]);
        remove_hooks(&mut settings, &owned);
        assert_eq!(
            settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "neomax orient --hook"
        );
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            1
        );
    }
}
