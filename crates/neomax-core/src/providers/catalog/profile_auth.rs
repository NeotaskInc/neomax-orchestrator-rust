use std::path::Path;

use crate::Engine;

use super::environment::Environment;
use super::filesystem::FileSystem;
use super::profile_auth_claude::{claude_auth, claude_credential_present};
use super::profile_auth_codex::codex_auth;
use super::profile_auth_grok::grok_auth;
use super::profile_auth_kimi::kimi_auth;
use super::profile_auth_opencode::{opencode_auth, opencode_auth_with_environment};
use super::profiles::credential_path;
use super::types::{AuthMethod, AuthStatus};

pub(super) fn detect_auth(
    engine: Engine,
    profile: &Path,
    home: &Path,
    filesystem: &dyn FileSystem,
) -> (AuthStatus, bool) {
    let methods = match engine {
        Engine::Claude => claude_auth(profile, filesystem),
        Engine::Codex => codex_auth(profile, filesystem),
        Engine::Opencode => opencode_auth(profile, home, filesystem),
        Engine::Kimi => kimi_auth(profile, filesystem),
        Engine::Grok => grok_auth(profile, filesystem),
    };
    let credential_present = credential_present(engine, profile, home, filesystem);
    let auth = if methods.is_empty() {
        AuthStatus::Unauthenticated
    } else {
        AuthStatus::Authenticated { methods }
    };
    (auth, credential_present)
}

pub(super) fn detect_auth_with_environment(
    engine: Engine,
    profile: &Path,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> (AuthStatus, bool) {
    let methods = match engine {
        Engine::Claude => claude_auth(profile, filesystem),
        Engine::Codex => codex_auth(profile, filesystem),
        Engine::Opencode => opencode_auth_with_environment(profile, environment, filesystem),
        Engine::Kimi => kimi_auth(profile, filesystem),
        Engine::Grok => grok_auth(profile, filesystem),
    };
    let credential_present =
        credential_present_with_environment(engine, profile, home, environment, filesystem);
    let auth = if methods.is_empty() {
        AuthStatus::Unauthenticated
    } else {
        AuthStatus::Authenticated { methods }
    };
    (auth, credential_present)
}

fn credential_present(
    engine: Engine,
    profile: &Path,
    home: &Path,
    filesystem: &dyn FileSystem,
) -> bool {
    match engine {
        Engine::Claude => claude_credential_present(profile, filesystem),
        Engine::Codex | Engine::Opencode | Engine::Grok => {
            filesystem.is_file(&credential_path(engine, profile, home))
        }
        Engine::Kimi => {
            filesystem.is_file(&credential_path(engine, profile, home))
                || filesystem.is_file(&profile.join("config.toml"))
        }
    }
}

fn credential_present_with_environment(
    engine: Engine,
    profile: &Path,
    home: &Path,
    environment: &dyn Environment,
    filesystem: &dyn FileSystem,
) -> bool {
    if engine == Engine::Opencode {
        return filesystem.is_file(&environment.opencode_data_dir(profile).join("auth.json"));
    }
    credential_present(engine, profile, home, filesystem)
}

pub(super) fn supports_in_place_rotation(engine: Engine, methods: &[AuthMethod]) -> bool {
    matches!(engine, Engine::Claude | Engine::Codex)
        && methods
            .iter()
            .any(|method| matches!(method, AuthMethod::OAuth | AuthMethod::Device))
}
