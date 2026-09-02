use crate::providers::ParsedEvents;

use super::request::refresh_request as build_refresh_request;
use super::types::{CodexQuotaRefreshRequest, CodexQuotaRefreshResult};

pub fn refresh_request(parsed: &ParsedEvents) -> Option<CodexQuotaRefreshRequest> {
    build_refresh_request(parsed)
}

pub fn codex_quota_refresh_request(parsed: &ParsedEvents) -> Option<CodexQuotaRefreshRequest> {
    refresh_request(parsed)
}

pub fn apply_refresh_result(parsed: &mut ParsedEvents, result: &CodexQuotaRefreshResult) {
    result.apply_to(parsed);
}

pub fn apply_codex_quota_refresh(parsed: &mut ParsedEvents, result: &CodexQuotaRefreshResult) {
    apply_refresh_result(parsed, result);
}
