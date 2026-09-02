use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use neomax_core::config::Engine;
use serde_json::Value;

use super::*;
use crate::test_support::agent_paths;

type HttpCall = (String, Vec<(String, String)>, Duration);
type PostCall = (String, Vec<(String, String)>, Value, Duration);

#[derive(Default)]
struct FakeHttp {
    calls: Mutex<Vec<HttpCall>>,
    post_calls: Mutex<Vec<PostCall>>,
    response: Value,
    post_response: Option<Value>,
}

impl FakeHttp {
    fn new(response: Value) -> Self {
        Self {
            response,
            ..Self::default()
        }
    }

    fn with_post_response(mut self, response: Value) -> Self {
        self.post_response = Some(response);
        self
    }
}

impl JsonHttp for FakeHttp {
    fn get_json(&self, url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<Value> {
        self.calls.lock().unwrap().push((
            url.into(),
            headers
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
            timeout,
        ));
        Ok(self.response.clone())
    }

    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        self.post_calls.lock().unwrap().push((
            url.into(),
            headers
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
            body.clone(),
            timeout,
        ));
        self.post_response
            .clone()
            .ok_or_else(|| anyhow::anyhow!("unexpected fake POST"))
    }
}

#[test]
fn claude_usage_is_parsed_and_cached_without_a_live_provider_call() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let profile = paths.home.join(".claude");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture-token","expiresAt":4102444800000}}"#,
    )
    .unwrap();
    let http = Arc::new(FakeHttp::new(serde_json::json!({
        "five_hour": {"utilization": 42.5, "resets_at": "2040-01-02T03:04:05Z"},
        "seven_day": {"utilization": 8.0, "resets_at": 4102444800_i64}
    })));
    let refresher = LocalQuotaRefresher::with_http(paths.clone(), http.clone());
    let report = refresher.refresh(true).unwrap();
    assert_eq!(report.errors, 0);
    assert_eq!(report.providers[0].engine, Engine::Claude);
    let cache = neomax_core::usage::UsageCacheStore::new(&paths.state.usage)
        .load(Engine::Claude, &profile)
        .unwrap();
    assert_eq!(cache.five_hour.used_percent, Some(42.5));
    assert_eq!(cache.seven_day.used_percent, Some(8.0));
    let calls = http.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, claude::CLAUDE_USAGE_URL);
    assert_eq!(calls[0].2, claude::CLAUDE_TIMEOUT);
    assert_eq!(
        calls[0].1,
        vec![
            ("Authorization".into(), "Bearer fixture-token".into()),
            ("anthropic-beta".into(), "oauth-2025-04-20".into()),
            ("anthropic-version".into(), "2023-06-01".into()),
        ]
    );
}

#[test]
fn refresh_reports_provider_failures_without_exposing_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let refresher =
        LocalQuotaRefresher::with_http(paths, Arc::new(FakeHttp::new(serde_json::json!({}))));
    let report = refresher.refresh(true).unwrap();
    let text = serde_json::to_string(&report).unwrap();
    assert!(!text.contains("fixture-token"));
    assert_eq!(report.providers[0].capability, QuotaSupport::Numeric);
    assert!(report.providers[0].attempted);
    assert!(!report.providers[0].refreshed);
}

#[test]
fn reactive_providers_do_not_attempt_numeric_quota_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let profile = paths.home.join(".opencode");
    std::fs::create_dir_all(&profile).unwrap();
    let http = Arc::new(FakeHttp::new(serde_json::json!({
        "five_hour": {"utilization": 17.0},
        "seven_day": {"utilization": 3.0}
    })));
    let refresher = LocalQuotaRefresher::with_http(paths, http.clone());

    let report = refresher.refresh_profile(Engine::Opencode, &profile, true);

    assert_eq!(report.capability, QuotaSupport::Reactive);
    assert!(!report.attempted);
    assert!(!report.refreshed);
    assert!(report.source.is_none());
    assert!(http.calls.lock().unwrap().is_empty());
}

#[test]
fn expired_claude_oauth_is_refreshed_and_persisted_with_injected_http() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let profile = paths.home.join(".claude");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"expired-token","refreshToken":"old-refresh","expiresAt":1}}"#,
    )
    .unwrap();
    let http = Arc::new(
        FakeHttp::new(serde_json::json!({
            "five_hour": {"utilization": 12.0},
            "seven_day": {"utilization": 4.0}
        }))
        .with_post_response(serde_json::json!({
            "access_token": "new-access",
            "refresh_token": "new-refresh",
            "expires_in": 3600
        })),
    );
    let refresher = LocalQuotaRefresher::with_http(paths.clone(), http.clone());

    let report = refresher.refresh(true).unwrap();
    assert_eq!(report.errors, 0);
    let credentials: Value =
        serde_json::from_slice(&std::fs::read(profile.join(".credentials.json")).unwrap()).unwrap();
    assert_eq!(
        credentials["claudeAiOauth"]["accessToken"],
        Value::String("new-access".into())
    );
    assert_eq!(
        credentials["claudeAiOauth"]["refreshToken"],
        Value::String("new-refresh".into())
    );
    let posts = http.post_calls.lock().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].0, claude::CLAUDE_OAUTH_TOKEN_URL);
    assert_eq!(
        posts[0].1,
        vec![
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "application/json".into()),
            (
                "User-Agent".into(),
                "claude-cli/2.1.56 (external, cli)".into()
            ),
        ]
    );
    assert_eq!(posts[0].2["grant_type"], "refresh_token");
    assert_eq!(posts[0].2["refresh_token"], "old-refresh");
    assert_eq!(
        posts[0].2["client_id"],
        "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
    );
    assert_eq!(posts[0].3, claude::CLAUDE_TIMEOUT);
}

#[test]
fn codex_rollout_quota_is_cached_from_local_jsonl_without_http() {
    let temp = tempfile::tempdir().unwrap();
    let paths = agent_paths(&temp);
    let sessions = paths.home.join(".codex").join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("rollout.jsonl"),
        r#"{"payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":88,"window_minutes":300,"resets_at":4102444800},"secondary":{"used_percent":45,"window_minutes":10080,"resets_at":4102444800}}}}
"#,
    )
    .unwrap();
    let http = Arc::new(FakeHttp::new(serde_json::json!({})));
    let refresher = LocalQuotaRefresher::with_http(paths.clone(), http.clone());

    let report = refresher.refresh(true).unwrap();
    let cache = neomax_core::usage::UsageCacheStore::new(&paths.state.usage)
        .load(Engine::Codex, &paths.home.join(".codex"))
        .unwrap();

    assert_eq!(report.errors, 0);
    assert!(
        report
            .providers
            .iter()
            .any(|provider| provider.engine == Engine::Codex && provider.refreshed)
    );
    assert_eq!(cache.five_hour.used_percent, Some(88.0));
    assert_eq!(cache.seven_day.used_percent, Some(45.0));
    assert_eq!(cache.seven_day.resets_at, Some(4_102_444_800.0));
    assert!(http.calls.lock().unwrap().is_empty());
}
