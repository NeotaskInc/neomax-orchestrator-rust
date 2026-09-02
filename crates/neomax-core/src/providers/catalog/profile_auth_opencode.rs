use std::path::Path;

use crate::Engine;
use serde_json::Value;

use super::environment::Environment;
use super::filesystem::FileSystem;
use super::profile_auth_common::{json_file, unique_methods};
use super::profile_auth_store::classify_store_entry;
use super::profiles::credential_path;
use super::types::AuthMethod;
pub(super) fn opencode_auth(
    profile: &Path,
    home: &Path,
    filesystem: &dyn FileSystem,
) -> Vec<AuthMethod> {
    let path = credential_path(Engine::Opencode, profile, home);
    opencode_auth_at(&path, filesystem)
}

pub(super) fn opencode_auth_with_environment(
    profile: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> Vec<AuthMethod> {
    let path = environment.opencode_data_dir(profile).join("auth.json");
    opencode_auth_at(&path, filesystem)
}

fn opencode_auth_at(path: &Path, filesystem: &dyn FileSystem) -> Vec<AuthMethod> {
    let Some(Value::Object(store)) = json_file(path.to_path_buf(), filesystem) else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    for value in store.values() {
        if let Some(object) = value.as_object() {
            methods.extend(classify_store_entry(object));
        }
    }
    unique_methods(methods)
}
