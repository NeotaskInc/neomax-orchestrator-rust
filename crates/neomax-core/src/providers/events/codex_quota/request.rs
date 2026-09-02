pub const CODEX_RATE_LIMIT_REFRESH_METHOD: &str = "account/rateLimits/read";
pub const CODEX_RATE_LIMIT_REFRESH_TIMEOUT_MS: u64 = 5_000;

use super::types::{CodexQuotaRefreshReason, CodexQuotaRefreshRequest};

pub(super) fn refresh_request(
    parsed: &crate::providers::ParsedEvents,
) -> Option<CodexQuotaRefreshRequest> {
    parsed.rate_limited.then(CodexQuotaRefreshRequest::default)
}

impl CodexQuotaRefreshRequest {
    pub fn bounded(timeout_ms: u64) -> Self {
        Self {
            method: CODEX_RATE_LIMIT_REFRESH_METHOD.into(),
            timeout_ms: timeout_ms.clamp(100, CODEX_RATE_LIMIT_REFRESH_TIMEOUT_MS),
            reason: CodexQuotaRefreshReason::UsageLimit,
        }
    }
}

impl Default for CodexQuotaRefreshRequest {
    fn default() -> Self {
        Self::bounded(CODEX_RATE_LIMIT_REFRESH_TIMEOUT_MS)
    }
}
