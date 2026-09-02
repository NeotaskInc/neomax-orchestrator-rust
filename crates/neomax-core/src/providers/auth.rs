use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::Duration;

use crate::Engine;
#[cfg(target_os = "macos")]
use crate::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
#[cfg(target_os = "macos")]
use crate::providers::scrub_provider_process_request;
use crate::runtime::RuntimeEnvironment;

use super::catalog::{self, AuthMethod, AuthStatus, RealFileSystem};
use super::{AuthState, ProviderProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KimiAuthMethod {
    OAuth,
    ApiKey,
}

pub fn current_auth_state(profile: &ProviderProfile) -> AuthState {
    let environment = RuntimeEnvironment::process();
    let Some(home) = environment.home_dir() else {
        return AuthState::Unknown;
    };
    auth_state(profile, &home)
}

pub fn auth_state(profile: &ProviderProfile, home: &Path) -> AuthState {
    let snapshot = catalog::inspect_profile_snapshot(
        profile.engine,
        profile.account.clone(),
        profile.path.clone(),
        profile.reserved,
        home,
        &RealFileSystem,
    );
    let authenticated = match profile.engine {
        Engine::Claude => {
            snapshot.eligibility.authenticated || claude_keychain_authenticated(&profile.path, home)
        }
        Engine::Kimi => snapshot.eligibility.authenticated,
        _ => snapshot.eligibility.authenticated,
    };
    if authenticated {
        AuthState::Authenticated
    } else {
        AuthState::Unauthenticated
    }
}

pub fn keychain_service(profile: &Path, home: &Path) -> String {
    catalog::claude_keychain_service(profile, home)
}

pub fn checked_keychain_service(profile: &Path, home: &Path) -> crate::Result<String> {
    catalog::checked_claude_keychain_service(profile, home)
}

pub fn opencode_auth_path(profile: &Path, home: &Path) -> PathBuf {
    let environment = RuntimeEnvironment::process();
    if environment.home_dir().as_deref() == Some(home) {
        return environment.opencode_auth_path(profile);
    }
    catalog::credential_path(Engine::Opencode, profile, home)
}

pub fn kimi_auth_method(profile: &Path) -> Option<KimiAuthMethod> {
    let home = profile.parent().unwrap_or_else(|| Path::new(""));
    let snapshot = catalog::inspect_profile_snapshot(
        Engine::Kimi,
        "unknown",
        profile.to_path_buf(),
        false,
        home,
        &RealFileSystem,
    );
    let AuthStatus::Authenticated { methods } = snapshot.auth else {
        return None;
    };
    methods.into_iter().find_map(|method| match method {
        AuthMethod::OAuth => Some(KimiAuthMethod::OAuth),
        AuthMethod::ApiKey => Some(KimiAuthMethod::ApiKey),
        AuthMethod::Device | AuthMethod::LocalCredential => None,
    })
}

#[cfg(target_os = "macos")]
fn claude_keychain_authenticated(profile: &Path, home: &Path) -> bool {
    let Ok(service) = checked_keychain_service(profile, home) else {
        return false;
    };
    let request = ProcessRequest::new("security")
        .args(["find-generic-password", "-s"])
        .arg(service)
        .timeout(Duration::from_secs(5))
        .stdout_limit(16 * 1024)
        .stderr_limit(16 * 1024);
    let request = scrub_provider_process_request(request);
    LocalProcessRunner::default()
        .capture(&request)
        .is_ok_and(|output| output.success && !output.timed_out && !output.stdout_truncated)
}

#[cfg(not(target_os = "macos"))]
fn claude_keychain_authenticated(_profile: &Path, _home: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn profile(engine: Engine, path: PathBuf) -> ProviderProfile {
        ProviderProfile {
            engine,
            account: "2".into(),
            path,
            reserved: false,
        }
    }

    #[test]
    fn detects_each_file_backed_auth_format_without_a_provider_call() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();

        let codex = home.join(".codex-acct2");
        fs::create_dir_all(&codex).unwrap();
        fs::write(
            codex.join("auth.json"),
            r#"{"tokens":{"access_token":"fixture"}}"#,
        )
        .unwrap();
        assert_eq!(
            auth_state(&profile(Engine::Codex, codex), home),
            AuthState::Authenticated
        );

        let opencode = home.join(".opencode-acct2/opencode");
        fs::create_dir_all(&opencode).unwrap();
        fs::write(
            opencode.join("auth.json"),
            r#"{"registry":{"key":"fixture"}}"#,
        )
        .unwrap();
        assert_eq!(
            auth_state(
                &profile(Engine::Opencode, home.join(".opencode-acct2")),
                home
            ),
            AuthState::Authenticated
        );

        let kimi = home.join(".kimi-code-acct2/credentials");
        fs::create_dir_all(&kimi).unwrap();
        fs::write(
            kimi.join("kimi-code.json"),
            r#"{"refresh_token":"fixture"}"#,
        )
        .unwrap();
        assert_eq!(
            auth_state(&profile(Engine::Kimi, home.join(".kimi-code-acct2")), home),
            AuthState::Authenticated
        );

        let grok = home.join(".grok-acct2");
        fs::create_dir_all(&grok).unwrap();
        fs::write(
            grok.join("auth.json"),
            r#"{"xai::oidc":{"auth_mode":"oidc","key":"fixture"}}"#,
        )
        .unwrap();
        assert_eq!(
            auth_state(&profile(Engine::Grok, grok), home),
            AuthState::Authenticated
        );
    }

    #[test]
    fn recognizes_kimi_api_keys_as_authenticated_profiles() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("config.toml"), "api_key = \"fixture\"\n").unwrap();
        assert_eq!(kimi_auth_method(temp.path()), Some(KimiAuthMethod::ApiKey));
        assert_eq!(
            auth_state(&profile(Engine::Kimi, temp.path().into()), temp.path()),
            AuthState::Authenticated
        );
    }
}
