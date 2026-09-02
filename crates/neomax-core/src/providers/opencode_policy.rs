use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::providers::catalog::OPENCODE_DEFAULT_MODEL;
use crate::{Error, Result};

const TEMPLATE: &str = include_str!("../../assets/opencode-model-policy.json");
const REQUIRED_AGENTS: [&str; 8] = [
    "build",
    "plan",
    "general",
    "explore",
    "scout",
    "compaction",
    "title",
    "summary",
];

pub fn content(model: &str) -> Result<String> {
    let mut policy: Value = serde_json::from_str(TEMPLATE)
        .map_err(|error| Error::Message(format!("invalid OpenCode policy template: {error}")))?;
    validate(&policy)?;
    let (provider, _) = model.split_once('/').ok_or_else(|| {
        Error::InvalidArgument(format!(
            "OpenCode model must use provider/model form: {model}"
        ))
    })?;
    if provider.is_empty() {
        return Err(Error::InvalidArgument(format!(
            "OpenCode model must use provider/model form: {model}"
        )));
    }
    let object = policy.as_object_mut().ok_or_else(|| {
        Error::Message("OpenCode policy template must contain a JSON object".into())
    })?;
    object.insert("model".into(), Value::String(model.into()));
    object.insert("small_model".into(), Value::String(model.into()));
    object.insert(
        "enabled_providers".into(),
        Value::Array(vec![Value::String(provider.into())]),
    );
    for item in agents_mut(object)?.values_mut() {
        item.as_object_mut()
            .ok_or_else(|| Error::Message("OpenCode agent policy must be an object".into()))?
            .insert("model".into(), Value::String(model.into()));
    }
    serde_json::to_string(&policy)
        .map_err(|error| Error::Message(format!("could not encode OpenCode policy: {error}")))
}

fn validate(policy: &Value) -> Result<()> {
    let object = policy
        .as_object()
        .ok_or_else(|| Error::Message("OpenCode policy template must be an object".into()))?;
    if object.get("model").and_then(Value::as_str) != Some(OPENCODE_DEFAULT_MODEL)
        || object.get("small_model").and_then(Value::as_str) != Some(OPENCODE_DEFAULT_MODEL)
    {
        return Err(Error::Message(
            "OpenCode policy template model pins do not match".into(),
        ));
    }
    if object.get("share").and_then(Value::as_str) != Some("disabled") {
        return Err(Error::Message(
            "OpenCode policy template must disable sharing".into(),
        ));
    }
    let providers = object
        .get("enabled_providers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Error::Message("OpenCode policy template has no provider allowlist".into())
        })?;
    let expected_provider = OPENCODE_DEFAULT_MODEL
        .split_once('/')
        .map(|(provider, _)| provider)
        .ok_or_else(|| Error::Message("OpenCode default model is not provider-qualified".into()))?;
    if providers != &[Value::String(expected_provider.into())] {
        return Err(Error::Message(
            "OpenCode policy template provider allowlist does not match".into(),
        ));
    }
    let agents = object
        .get("agent")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::Message("OpenCode policy template has no agent map".into()))?;
    let names = agents.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if names != REQUIRED_AGENTS.into_iter().collect() {
        return Err(Error::Message(
            "OpenCode policy template agent set does not match".into(),
        ));
    }
    if agents
        .values()
        .any(|agent| agent.get("model").and_then(Value::as_str) != Some(OPENCODE_DEFAULT_MODEL))
    {
        return Err(Error::Message(
            "OpenCode policy template must pin every agent".into(),
        ));
    }
    Ok(())
}

fn agents_mut(object: &mut Map<String, Value>) -> Result<&mut Map<String, Value>> {
    object
        .get_mut("agent")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| Error::Message("OpenCode policy template has no agent map".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repins_every_native_agent_and_provider() {
        let policy: Value = serde_json::from_str(&content("opencode/big-pickle").unwrap()).unwrap();
        assert_eq!(policy["model"], "opencode/big-pickle");
        assert_eq!(policy["small_model"], "opencode/big-pickle");
        assert_eq!(policy["enabled_providers"], serde_json::json!(["opencode"]));
        assert_eq!(policy["share"], "disabled");
        assert!(
            policy["agent"]
                .as_object()
                .unwrap()
                .values()
                .all(|agent| agent["model"] == "opencode/big-pickle")
        );
    }
}
