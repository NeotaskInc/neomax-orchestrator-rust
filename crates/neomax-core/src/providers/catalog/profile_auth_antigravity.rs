//! Antigravity CLI (`agy`) sign-in detection.
//!
//! agy stores its Google sign-in in the operating system keyring, which
//! Neomax never reads. A profile counts as connected once agy has initialised
//! its application data there (`antigravity-cli/installation_id`) and has not
//! recorded a sign-out, or when its settings select Gemini API-key mode.

use std::path::Path;

use serde_json::Value;

use super::filesystem::FileSystem;
use super::profile_auth_common::json_file;
use super::types::AuthMethod;

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
