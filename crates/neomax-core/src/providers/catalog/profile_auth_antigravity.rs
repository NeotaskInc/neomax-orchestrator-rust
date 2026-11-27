//! Antigravity CLI (`agy`) sign-in detection.
//!
//! agy stores its Google sign-in in the operating system keyring, which
//! Neomax never reads. A profile is a sign-in candidate once agy has
//! initialised its application data (`antigravity-cli/installation_id`), or
//! connected when its settings select Gemini API-key mode. Discovery then
//! confirms a native sign-in with the `agy models` run it already makes for
//! model listing: agy exits non-zero and asks to sign in when it is signed out.

use std::path::Path;

use serde_json::Value;

use super::commands::CommandOutput;
use super::filesystem::FileSystem;
use super::profile_auth_common::json_file;
use super::types::{AuthMethod, AuthStatus, ProfileSnapshot};

pub(super) const APP_DATA_DIR: &str = "antigravity-cli";
const INSTALLATION_MARKER: &str = "installation_id";
const SETTINGS_FILE: &str = "settings.json";

pub(super) fn antigravity_auth(profile: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let app_data = profile.join(APP_DATA_DIR);
    if api_key_mode(&app_data, filesystem) {
        return vec![AuthMethod::ApiKey];
    }
    if filesystem.is_file(&app_data.join(INSTALLATION_MARKER)) {
        return vec![AuthMethod::OAuth];
    }
    Vec::new()
}

pub(super) fn settings_path(profile: &Path) -> std::path::PathBuf {
    profile.join(APP_DATA_DIR).join(SETTINGS_FILE)
}

fn api_key_mode(app_data: &Path, filesystem: &dyn FileSystem) -> bool {
    json_file(app_data.join(SETTINGS_FILE), filesystem)
        .as_ref()
        .and_then(|settings| settings.get("modelProvider"))
        .and_then(Value::as_str)
        .is_some_and(|provider| provider.eq_ignore_ascii_case("gemini"))
}

/// Mark native-sign-in profiles signed out when agy's own model listing
/// completed and failed. A timeout or truncated answer keeps the file evidence.
pub(super) fn apply_sign_in_probe(profiles: &mut [ProfileSnapshot], listing: &CommandOutput) {
    if listing.success || listing.timed_out || listing.truncated {
        return;
    }
    for profile in profiles {
        let native_only = matches!(
            &profile.auth,
            AuthStatus::Authenticated { methods } if methods == &[AuthMethod::OAuth]
        );
        if native_only {
            profile.auth = AuthStatus::Unauthenticated;
            profile.eligibility.authenticated = false;
            profile.eligibility.worker_eligible = false;
            profile.eligibility.orchestrator_eligible = false;
            profile.eligibility.rotation_eligible = false;
            profile.eligibility.managed_pool_eligible = false;
        }
    }
}
