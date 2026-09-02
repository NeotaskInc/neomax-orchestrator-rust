use neomax_core::usage::UsageMetrics;

pub(crate) fn window_cutoff(days: u32, now: i64) -> i64 {
    if days == 0 {
        0
    } else {
        now.saturating_sub(i64::from(days) * 86_400)
    }
}

pub(crate) fn subtract(total: &UsageMetrics, subtract: &UsageMetrics) -> UsageMetrics {
    UsageMetrics {
        input: total.input.saturating_sub(subtract.input),
        output: total.output.saturating_sub(subtract.output),
        reasoning: total.reasoning.saturating_sub(subtract.reasoning),
        cache_write: total.cache_write.saturating_sub(subtract.cache_write),
        cache_read: total.cache_read.saturating_sub(subtract.cache_read),
        requests: total.requests.saturating_sub(subtract.requests),
        completions: total.completions.saturating_sub(subtract.completions),
        unfinished: total.unfinished.saturating_sub(subtract.unfinished),
        errors: total.errors.saturating_sub(subtract.errors),
        rate_limits: total.rate_limits.saturating_sub(subtract.rate_limits),
        cost: (total.cost - subtract.cost).max(0.0),
    }
}

pub(crate) fn add(total: &mut UsageMetrics, other: &UsageMetrics) {
    total.input = total.input.saturating_add(other.input);
    total.output = total.output.saturating_add(other.output);
    total.reasoning = total.reasoning.saturating_add(other.reasoning);
    total.cache_write = total.cache_write.saturating_add(other.cache_write);
    total.cache_read = total.cache_read.saturating_add(other.cache_read);
    total.requests = total.requests.saturating_add(other.requests);
    total.completions = total.completions.saturating_add(other.completions);
    total.unfinished = total.unfinished.saturating_add(other.unfinished);
    total.errors = total.errors.saturating_add(other.errors);
    total.rate_limits = total.rate_limits.saturating_add(other.rate_limits);
    total.cost += other.cost;
}
