use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::DateTime;
use regex::Regex;
use serde_json::Value;

use super::json::number_value;

pub(super) static LIMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)rate.?limit|usage limit|too many requests|tokens per min|\btpm\b|requests per min|\b429\b|quota|credit balance is too low|credits? (?:depleted|exhausted)|please try again in \d+(?:ms|s)").unwrap()
});

pub(super) fn reset_epoch(headers: &Value, now: f64) -> Option<f64> {
    let headers = headers.as_object()?;
    let normalized = headers
        .iter()
        .map(|(key, value)| (key.to_ascii_lowercase(), value))
        .collect::<BTreeMap<_, _>>();
    for key in ["retry-after-ms", "x-ratelimit-reset-after-ms"] {
        if let Some(value) = normalized.get(key).and_then(|value| number_value(value)) {
            return Some(now + value / 1000.0);
        }
    }
    if let Some(value) = normalized.get("retry-after") {
        if let Some(seconds) = number_value(value) {
            return Some(now + seconds);
        }
        if let Some(date) = value.as_str().and_then(parse_http_date) {
            return Some(date);
        }
    }
    for key in ["x-ratelimit-reset", "ratelimit-reset"] {
        if let Some(value) = normalized.get(key).and_then(|value| number_value(value)) {
            return normalize_reset(value, now);
        }
    }
    None
}

pub(super) fn epoch_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn normalize_reset(mut value: f64, now: f64) -> Option<f64> {
    if value <= 0.0 {
        return None;
    }
    while value > 100_000_000_000.0 {
        value /= 1000.0;
    }
    (value > now && value <= now + 7.0 * 24.0 * 3600.0 + 3600.0).then_some(value)
}

fn parse_http_date(value: &str) -> Option<f64> {
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|date| date.timestamp_millis() as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reset_headers_support_milliseconds_seconds_and_absolute_epochs() {
        assert_eq!(
            reset_epoch(&json!({"Retry-After-MS": 250}), 100.0),
            Some(100.25)
        );
        assert_eq!(
            reset_epoch(&json!({"retry-after": "2.5"}), 100.0),
            Some(102.5)
        );
        assert_eq!(
            reset_epoch(&json!({"x-ratelimit-reset": 105.0}), 100.0),
            Some(105.0)
        );
    }

    #[test]
    fn reset_headers_accept_http_dates_and_reject_stale_or_unbounded_values() {
        let date = "Wed, 21 Oct 2015 07:28:00 GMT";
        assert_eq!(
            reset_epoch(&json!({"retry-after": date}), 1_440_000_000.0),
            Some(1_445_412_480.0)
        );
        assert_eq!(reset_epoch(&json!({"ratelimit-reset": 99.0}), 100.0), None);
        assert_eq!(
            reset_epoch(&json!({"ratelimit-reset": 100_000_000_000_000.0}), 100.0),
            None
        );
    }
}
