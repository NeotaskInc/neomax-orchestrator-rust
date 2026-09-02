use std::path::{Path, PathBuf};
use std::time::Duration;

use neomax_core::config::Engine;
use neomax_core::providers::CodexQuotaRefreshResult;
use neomax_core::usage::{ProviderUsageCache, QuotaWindow};
use walkdir::WalkDir;

use super::cache::load;
use super::cache::{CODEX_FRESH_SECS, DEFAULT_STALE_SECS, fresh, now_epoch, stale_ok};
use crate::config::AgentPaths;
use crate::io::{file_len, read_range};

const TAIL_BYTES: u64 = 4 * 1024 * 1024;
const FIVE_HOUR_WINDOW_MAX_MINUTES: f64 = 360.0;

pub(crate) fn refresh(
    paths: &AgentPaths,
    profile: &Path,
    force: bool,
) -> Option<ProviderUsageCache> {
    let store = neomax_core::usage::UsageCacheStore::new(&paths.state.usage);
    let now = now_epoch();
    let cached = load(&store, Engine::Codex, profile);
    if !force
        && cached.as_ref().is_some_and(|cache| {
            fresh(
                cache,
                now,
                "codex-rollout",
                Duration::from_secs(CODEX_FRESH_SECS as u64),
            )
        })
    {
        return cached;
    }
    let live = latest_rollout(profile).and_then(|path| read_window(&path, now));
    if let Some(live) = live {
        return Some(live);
    }
    cached.and_then(|mut cache| {
        stale_ok(&cache, now, Duration::from_secs(DEFAULT_STALE_SECS as u64)).then(|| {
            cache.stale = true;
            cache
        })
    })
}

fn latest_rollout(profile: &Path) -> Option<PathBuf> {
    WalkDir::new(profile.join("sessions"))
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path().to_path_buf()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

fn read_window(path: &Path, now: i64) -> Option<ProviderUsageCache> {
    let size = file_len(path).ok()?;
    let start = size.saturating_sub(TAIL_BYTES);
    let length = usize::try_from(size.saturating_sub(start)).ok()?;
    let bytes = read_range(path, start, length).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut windows: [Option<(QuotaWindow, Option<f64>)>; 2] = [None, None];
    for line in text.lines().rev() {
        if !line.contains("token_count") || !line.contains("rate_limits") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let payload = value.get("payload").unwrap_or(&value);
        if payload.get("type").and_then(|value| value.as_str()) != Some("token_count") {
            continue;
        }
        let Some(snapshot) = CodexQuotaRefreshResult::from_value(value, now as f64) else {
            continue;
        };
        for (index, window) in [snapshot.primary, snapshot.secondary]
            .into_iter()
            .enumerate()
        {
            let Some(window) = window else { continue };
            let Some(used) = window.used_percent else {
                continue;
            };
            let candidate = (
                QuotaWindow {
                    used_percent: Some(used),
                    resets_at: window.resets_at,
                },
                window.window_minutes.map(|minutes| minutes as f64),
            );
            let replace = windows[index]
                .as_ref()
                .is_none_or(|(existing, _)| existing.used_percent.unwrap_or(0.0) < used);
            if replace {
                windows[index] = Some(candidate);
            }
        }
        if windows.iter().any(Option::is_some) {
            break;
        }
    }
    let mut five = None;
    let mut seven = None;
    for value in windows.into_iter() {
        let Some((value, minutes)) = value else {
            continue;
        };
        if minutes.is_none_or(|minutes| minutes <= FIVE_HOUR_WINDOW_MAX_MINUTES) {
            five = max_window(five, value);
        } else {
            seven = max_window(seven, value);
        }
    }
    if five.is_some() && seven.is_none() {
        seven = five.take();
    }
    Some(ProviderUsageCache {
        five_hour: five.unwrap_or_default(),
        seven_day: seven.unwrap_or_default(),
        source: Some("codex-rollout".into()),
        observed_at: Some(now as f64),
        ..ProviderUsageCache::default()
    })
}

fn max_window(left: Option<QuotaWindow>, right: QuotaWindow) -> Option<QuotaWindow> {
    if left
        .as_ref()
        .and_then(|window| window.used_percent)
        .unwrap_or(0.0)
        >= right.used_percent.unwrap_or(0.0)
    {
        left
    } else {
        Some(right)
    }
}
