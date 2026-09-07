use std::path::PathBuf;

use crate::{Engine, Error, Result};

use super::profile_identity::{canonical_email, profile_email_with_environment};
use super::{
    Environment, FileSystem, ProfileSelector, ProfileSnapshot, resolve_profile_path,
    resolve_profile_selector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub account: String,
    pub path: PathBuf,
    pub login_required: bool,
    pub expected_email: Option<String>,
    pub create_new: bool,
}

pub fn resolve_profile_entry(
    engine: Engine,
    profiles: &[ProfileSnapshot],
    selector: Option<&str>,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Result<Option<ProfileEntry>> {
    let home = environment.home_dir().ok_or_else(|| {
        Error::InvalidArgument("account selection requires a home directory".into())
    })?;
    if selector.is_none()
        && profiles.iter().any(|profile| {
            profile.engine == engine && profile.eligibility.authenticated && !profile.reserved
        })
    {
        return Ok(None);
    }
    let selector = selector.unwrap_or("1");
    let email = canonical_email(selector);
    let selected =
        resolve_profile_selector(engine, profiles, selector, &home, environment, filesystem);
    match selected {
        Ok(profile) => {
            return Ok(Some(ProfileEntry {
                account: profile.account.clone(),
                path: profile.path.clone(),
                login_required: !profile.eligibility.authenticated,
                expected_email: email,
                create_new: false,
            }));
        }
        Err(Error::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    if let Some(email) = email {
        for number in 1..=10_000 {
            let path = resolve_profile_path(engine, ProfileSelector::Number(number), environment)?;
            if !filesystem.is_dir(&path)
                && !filesystem.is_file(&path)
                && !profiles.iter().any(|profile| profile.path == path)
            {
                return Ok(Some(ProfileEntry {
                    account: number.to_string(),
                    path,
                    login_required: true,
                    expected_email: Some(email),
                    create_new: true,
                }));
            }
        }
        return Err(Error::Conflict(
            "no unused account profile slot is available".into(),
        ));
    }
    let profile_selector =
        if selector.eq_ignore_ascii_case("orch") || selector.eq_ignore_ascii_case("orchestrator") {
            ProfileSelector::Orchestrator
        } else if let Some(number) = selector.parse::<u32>().ok().filter(|number| *number > 0) {
            ProfileSelector::Number(number)
        } else {
            let alias = if selector.starts_with("alias:") {
                selector.to_owned()
            } else {
                format!("alias:{selector}")
            };
            let profile =
                resolve_profile_selector(engine, profiles, &alias, &home, environment, filesystem)?;
            return Ok(Some(ProfileEntry {
                account: profile.account.clone(),
                path: profile.path.clone(),
                login_required: !profile.eligibility.authenticated,
                expected_email: None,
                create_new: false,
            }));
        };
    let path = resolve_profile_path(engine, profile_selector, environment)?;
    let account = match profile_selector {
        ProfileSelector::Number(number) => number.to_string(),
        ProfileSelector::Orchestrator => "orch".into(),
    };
    Ok(Some(ProfileEntry {
        account,
        path,
        login_required: true,
        expected_email: None,
        create_new: false,
    }))
}

pub fn verify_profile_entry_identity(
    engine: Engine,
    entry: &ProfileEntry,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Result<()> {
    let Some(expected) = entry.expected_email.as_deref() else {
        return Ok(());
    };
    let home = environment.home_dir().ok_or_else(|| {
        Error::InvalidArgument("account selection requires a home directory".into())
    })?;
    let actual =
        profile_email_with_environment(engine, &entry.path, &home, environment, filesystem);
    if actual.as_deref() != Some(expected) {
        return Err(Error::Conflict(format!(
            "login did not confirm the requested email; no session was launched. Check account {} with `neomax {engine} whoami {}`",
            entry.account, entry.account
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
