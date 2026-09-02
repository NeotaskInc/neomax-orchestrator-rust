use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::auth::RotationPaths;
use neomax_core::providers::ProviderRuntime;

use crate::context::RuntimeContext;

pub(crate) fn rotation_paths(context: &RuntimeContext) -> RotationPaths {
    RotationPaths::new(
        context.paths.auth_backups.clone(),
        context.paths.auth_rotations.clone(),
    )
    .with_usage_cache_dir(&context.paths.usage)
}

pub(crate) fn resolve_profile(
    runtime: Option<&ProviderRuntime>,
    engine: Engine,
    selector: &str,
    home: &Path,
) -> Result<PathBuf> {
    let selector_path = Path::new(selector);
    if is_rooted_but_not_absolute(selector_path) {
        bail!(
            "cannot resolve {engine} profile {selector:?}: profile path must not be rooted without an absolute prefix"
        );
    }
    if is_rooted_but_not_absolute(home) {
        bail!(
            "cannot resolve {engine} profile: profile home must not be rooted without an absolute prefix"
        );
    }
    let explicit_path =
        if selector_path.is_absolute() || selector.contains('/') || selector.contains('\\') {
            Some(if selector_path.is_absolute() {
                selector_path.to_path_buf()
            } else {
                std::env::current_dir()?.join(selector_path)
            })
        } else {
            None
        };
    if explicit_path.is_none() {
        let home_path = home.join(selector);
        if home_path.exists() {
            return Ok(home_path);
        }
    }
    let profiles = runtime
        .map(|runtime| runtime.registry().profiles_for(engine))
        .transpose()?
        .unwrap_or_default();
    if let Some(path) = explicit_path {
        if is_rooted_but_not_absolute(&path) {
            bail!(
                "cannot resolve {engine} profile {selector:?}: profile path must not be rooted without an absolute prefix"
            );
        }
        if profiles.iter().any(|profile| profile.path == path) || path.exists() {
            return Ok(path);
        }
        return Ok(path);
    }
    profiles
        .into_iter()
        .filter(|profile| !is_rooted_but_not_absolute(&profile.path))
        .find(|profile| {
            profile.account.eq_ignore_ascii_case(selector)
                || (selector.eq_ignore_ascii_case("orch") && profile.reserved)
                || profile
                    .path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(selector))
                || profile.path == home.join(selector)
        })
        .map(|profile| profile.path)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve {engine} profile {selector:?}; use an account, orch, or profile path"
            )
        })
}

pub(crate) fn ensure_rotation_profile(
    runtime: Option<&ProviderRuntime>,
    engine: Engine,
    path: &Path,
    role: &str,
) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        bail!(
            "rotate-auth {role} profile {} must not be rooted without an absolute prefix",
            path.display()
        );
    }
    let Some(runtime) = runtime else {
        bail!(
            "rotate-auth cannot verify the {role} profile without provider discovery; use an account selector"
        );
    };
    let Some(profile) = runtime
        .registry()
        .profiles_for(engine)?
        .into_iter()
        .find(|profile| profile.path == path)
    else {
        if role == "destination" {
            return Ok(());
        }
        bail!(
            "rotate-auth {role} profile {} was not discovered as an authenticated profile",
            path.display()
        );
    };
    if !runtime.registry().rotation_eligible(&profile) {
        bail!(
            "rotate-auth {role} profile {} does not use OAuth or device credentials; API-key profiles use isolated provider handoff",
            path.display()
        );
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn rejects_windows_partial_root_selectors_before_home_joining() {
        let home = Path::new(r"C:\Users\fixture");
        for selector in [r"\rooted", r"C:drive-relative"] {
            let error = resolve_profile(None, Engine::Claude, selector, home).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn preserves_valid_absolute_profile_selectors() {
        let path = PathBuf::from(r"C:\profiles\claude-1");
        assert_eq!(
            resolve_profile(
                None,
                Engine::Claude,
                path.to_string_lossy().as_ref(),
                Path::new(r"C:\Users\fixture")
            )
            .unwrap(),
            path
        );
    }
}
