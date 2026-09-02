use neomax_core::sessions::SessionRecord;
use neomax_core::usage::LocalErrorView;
use serde_json::Value;

pub(crate) fn update_session_error(
    last_error: &mut Option<LocalErrorView>,
    record: &SessionRecord,
) {
    if record.errors == 0 && record.rate_limits == 0 {
        return;
    }
    let at = record.last_active.unwrap_or_default().max(0);
    if last_error.as_ref().is_some_and(|current| current.at > at) {
        return;
    }
    *last_error = Some(LocalErrorView {
        name: if record.rate_limits > 0 {
            "RateLimitError".into()
        } else {
            "LocalProviderError".into()
        },
        status: (record.rate_limits > 0).then(|| "429".into()),
        message: if record.rate_limits > 0 {
            "local provider reported a rate limit".into()
        } else {
            "local provider reported a request error".into()
        },
        at,
    });
}

pub(crate) fn local_error(value: &Value, at: i64) -> Option<LocalErrorView> {
    let object = value.as_object()?;
    let data = object.get("data").and_then(Value::as_object);
    let status = data
        .and_then(|data| data.get("statusCode").or_else(|| data.get("status")))
        .or_else(|| object.get("statusCode").or_else(|| object.get("status")))
        .map(value_text);
    let message = data
        .and_then(|data| data.get("message"))
        .or_else(|| object.get("message"))
        .or_else(|| object.get("name"))
        .map(value_text)
        .unwrap_or_else(|| "request failed".into());
    Some(LocalErrorView {
        name: object
            .get("name")
            .map(value_text)
            .unwrap_or_else(|| "Error".into()),
        status,
        message,
        at,
    })
}

fn value_text(value: &Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

pub(crate) fn safe_error(error: &str) -> String {
    crate::security::log_internal("local usage source", &error);
    "usage data unavailable".into()
}
