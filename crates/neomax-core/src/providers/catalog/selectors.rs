use std::path::Path;

use crate::{Engine, Error, Result};

use super::environment::Environment;
use super::filesystem::FileSystem;
use super::profile_identity::{canonical_email, profile_email_with_environment};
use super::types::ProfileSnapshot;

const ALIAS_PREFIX: &str = "alias:";

/// Resolve one discovered provider profile by its stable account ID, exact
/// local email, or an explicitly marked profile-directory alias.
pub fn resolve_profile_selector<'a>(
    engine: Engine,
    profiles: &'a [ProfileSnapshot],
    selector: &str,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Result<&'a ProfileSnapshot> {
    let matches = profiles
        .iter()
        .filter(|profile| {
            profile.engine == engine
                && profile_matches_selector(
                    profile,
                    selector,
                    home,
                    environment,
                    filesystem,
                )
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Err(Error::NotFound(format!(
            "{engine} profile selector {selector}"
        ))),
        [profile] => Ok(profile),
        profiles => Err(Error::Conflict(format!(
            "{engine} profile selector {selector} is ambiguous; matching profile IDs: {}",
            profiles
                .iter()
                .map(|profile| profile.account.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Test a selector against one discovered account without making an auth
/// decision. Callers apply their own routing or login eligibility policy.
fn profile_matches_selector(
    profile: &ProfileSnapshot,
    selector: &str,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }
    if let Some(alias) = selector.strip_prefix(ALIAS_PREFIX) {
        let alias = alias.trim();
        return !alias.is_empty() && profile_alias_matches(&profile.path, home, alias);
    }
    profile.account.eq_ignore_ascii_case(selector)
        || (profile.reserved
            && (selector.eq_ignore_ascii_case("orch")
                || selector.eq_ignore_ascii_case("orchestrator")))
        || canonical_email(selector).is_some_and(|requested| {
            profile_email_with_environment(profile.engine, &profile.path, home, environment, filesystem)
                .is_some_and(|email| email == requested)
        })
}

fn profile_alias_matches(profile: &Path, home: &Path, alias: &str) -> bool {
    profile
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(alias))
        || profile
            .strip_prefix(home)
            .ok()
            .and_then(|relative| relative.to_str())
            .is_some_and(|relative| relative.eq_ignore_ascii_case(alias))
}

#[cfg(test)]
#[path = "selectors_tests.rs"]
mod tests;
