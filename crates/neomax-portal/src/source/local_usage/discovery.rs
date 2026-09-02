use anyhow::Result;
use neomax_core::config::Engine;
use neomax_core::sessions::{DiscoveryContext, FsArtifactSource, SessionRecord, grok, kimi};

use super::metrics::window_cutoff;
use crate::source::FilesystemPortalSource;

pub(crate) fn discover(
    source: &FilesystemPortalSource,
    days: u32,
    now: i64,
) -> Result<Vec<SessionRecord>> {
    let cutoff = window_cutoff(days, now);
    let context = DiscoveryContext::new(now);
    let artifacts = FsArtifactSource::new(source.max_artifact_bytes);
    let mut records = Vec::new();
    for engine in [Engine::Kimi, Engine::Grok] {
        for profile in source.provider_profiles(engine)? {
            let discovered = match engine {
                Engine::Kimi => kimi::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
                Engine::Grok => grok::discover(
                    &artifacts,
                    &profile.path,
                    &profile.account,
                    &context,
                    cutoff,
                ),
                _ => unreachable!(),
            };
            if let Ok(rows) = discovered {
                records.extend(rows);
            }
        }
    }
    Ok(records)
}
