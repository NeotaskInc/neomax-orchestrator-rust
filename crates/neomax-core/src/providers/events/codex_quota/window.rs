use super::types::{CodexQuotaRefreshResult, CodexQuotaWindow};

pub(super) fn raw_reset(result: &CodexQuotaRefreshResult) -> Option<(f64, Option<String>)> {
    [result.primary.as_ref(), result.secondary.as_ref()]
        .into_iter()
        .filter_map(|window| {
            let window = window?;
            Some((window.resets_at?, window.window_minutes.map(window_name)))
        })
        .max_by(|left, right| left.0.total_cmp(&right.0))
}

pub(super) fn select_reset(
    primary: Option<&CodexQuotaWindow>,
    secondary: Option<&CodexQuotaWindow>,
    now: f64,
) -> (Option<f64>, Option<String>) {
    let windows = [(primary, "primary"), (secondary, "secondary")];
    let exhausted = windows
        .iter()
        .filter_map(|(window, _)| {
            window.filter(|window| window.used_percent.unwrap_or(0.0) >= 99.0)
        })
        .filter_map(|window| {
            window
                .resets_at
                .filter(|reset| *reset > now)
                .map(|reset| (reset, window.window_minutes))
        });
    let available = windows.iter().filter_map(|(window, _)| {
        window.and_then(|window| {
            window
                .resets_at
                .filter(|reset| *reset > now)
                .map(|reset| (reset, window.window_minutes))
        })
    });
    let selected = exhausted
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .or_else(|| available.min_by(|left, right| left.0.total_cmp(&right.0)));
    selected.map_or((None, None), |(reset, minutes)| {
        (Some(reset), minutes.map(window_name))
    })
}

fn window_name(minutes: u64) -> String {
    if (240..=360).contains(&minutes) {
        "five_hour".into()
    } else if minutes == 1_440 {
        "daily".into()
    } else if minutes >= 10_080 {
        "weekly".into()
    } else {
        format!("{minutes}m")
    }
}
