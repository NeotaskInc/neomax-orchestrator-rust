mod accounting;
mod discovery;
mod errors;
mod metrics;
mod opencode;
mod sessions;
mod tools;

#[cfg(test)]
mod tests;

use anyhow::Result;
use neomax_core::config::Engine;
use neomax_core::usage::ProviderUsageDetail;

use super::FilesystemPortalSource;

pub(crate) fn read_details(
    source: &FilesystemPortalSource,
    days: u32,
    now: i64,
) -> Result<[Vec<ProviderUsageDetail>; 3]> {
    let cutoff = metrics::window_cutoff(days, now);
    let session_records = discovery::discover(source, days, now)?;
    let mut details = [Vec::new(), Vec::new(), Vec::new()];
    for (index, engine) in [Engine::Opencode, Engine::Kimi, Engine::Grok]
        .into_iter()
        .enumerate()
    {
        for profile in source.provider_profiles(engine)? {
            let detail = match engine {
                Engine::Opencode => opencode::detail(source, &profile, days, cutoff),
                Engine::Kimi | Engine::Grok => {
                    sessions::detail(engine, &profile, days, &session_records, cutoff)
                }
                Engine::Claude | Engine::Codex => unreachable!(),
            };
            details[index].push(detail);
        }
    }
    Ok(details)
}
