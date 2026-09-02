use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use neomax_core::accounts::{QuotaSupport, quota_support};
use neomax_core::config::Engine;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::providers::catalog::{self, Environment};
use neomax_core::runtime::RuntimePlatform;
use serde_json::Value;

use crate::model::{EngineCapabilitiesView, QuotaCapabilityView};

pub(crate) fn capabilities_for(
    engine: Engine,
    home: &Path,
    usage: &Option<Value>,
    _telemetry: &Value,
    environment: &dyn Environment,
) -> EngineCapabilitiesView {
    let spec = catalog::spec(engine);
    let binary = binary_available(&spec.default_binary, &spec.binary_env, environment);
    let numeric_quota = matches!(quota_support(engine), QuotaSupport::Numeric);
    let windows = if numeric_quota {
        ["five_hour", "seven_day"]
            .into_iter()
            .filter(|key| {
                usage
                    .as_ref()
                    .and_then(|value| value.get(*key))
                    .and_then(|value| value.get("used_percent"))
                    .and_then(number)
                    .is_some()
            })
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let windows_available = !windows.is_empty();
    let source = if windows_available {
        usage
            .as_ref()
            .and_then(|value| value.get("source"))
            .and_then(Value::as_str)
            .unwrap_or("provider-cache")
            .to_string()
    } else {
        String::new()
    };
    let profile_root = if safe_absolute(home) {
        home.join(&spec.default_profile_dir)
    } else {
        PathBuf::new()
    };
    EngineCapabilitiesView {
        binary_available: binary,
        orchestrator: spec.capabilities.orchestrator,
        worker: spec.capabilities.worker,
        multiple_profiles: spec.capabilities.multiple_profiles,
        native_sessions: spec.capabilities.native_sessions,
        usage_discovery: spec.capabilities.usage_discovery,
        model_discovery: format!("{:?}", spec.capabilities.model_discovery),
        profile_root,
        quota: QuotaCapabilityView {
            supported: numeric_quota,
            available: numeric_quota && windows_available,
            source: (!source.is_empty()).then_some(source),
            windows,
            reactive: matches!(quota_support(engine), QuotaSupport::Reactive),
        },
    }
}

fn binary_available(default_binary: &str, binary_env: &str, environment: &dyn Environment) -> bool {
    let requested = environment
        .value(binary_env)
        .unwrap_or_else(|| default_binary.into());
    if requested.is_empty() || requested.chars().any(char::is_control) {
        return false;
    }
    let platform = environment.platform();
    let requested_path = PathBuf::from(&requested);
    if is_partial_root(&requested_path, &requested, platform) {
        return false;
    }
    if is_path_like(&requested) {
        return absolute_for_platform(&requested_path, &requested, platform)
            && requested_path.is_file();
    }

    if let Ok(resolved) = environment.resolve_provider_executable(&requested) {
        if let Some(path) = resolved.resolved_path {
            if absolute_for_platform(&path, &path.to_string_lossy(), platform) && path.is_file() {
                return true;
            }
        }
        let path = PathBuf::from(&resolved.program);
        if absolute_for_platform(&path, &path.to_string_lossy(), platform) && path.is_file() {
            return true;
        }
    }

    let Some(path_value) = environment.value("PATH") else {
        return false;
    };
    path_entries(&path_value, platform)
        .into_iter()
        .filter(|directory| {
            absolute_for_platform(directory, &directory.to_string_lossy(), platform)
        })
        .map(|directory| directory.join(&requested_path))
        .any(|path| path.is_file())
}

fn safe_absolute(path: &Path) -> bool {
    path.is_absolute() && !is_rooted_but_not_absolute(path)
}

fn is_path_like(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute() || value.contains('/') || value.contains('\\')
}

fn is_partial_root(path: &Path, raw: &str, platform: RuntimePlatform) -> bool {
    is_rooted_but_not_absolute(path)
        || (platform.is_windows()
            && ((raw.starts_with('\\') && !raw.starts_with("\\\\"))
                || (raw.starts_with('/') && !raw.starts_with("//"))
                || (raw.as_bytes().get(1) == Some(&b':')
                    && !raw
                        .as_bytes()
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))))
}

fn absolute_for_platform(path: &Path, raw: &str, platform: RuntimePlatform) -> bool {
    !is_partial_root(path, raw, platform)
        && (path.is_absolute()
            || (platform.is_windows()
                && (raw.starts_with("\\\\")
                    || raw.starts_with("//")
                    || (raw.as_bytes().get(1) == Some(&b':')
                        && raw
                            .as_bytes()
                            .get(2)
                            .is_some_and(|separator| *separator == b'/' || *separator == b'\\')))))
}

fn path_entries(raw: &str, platform: RuntimePlatform) -> Vec<PathBuf> {
    if platform.is_windows() {
        raw.split(';').map(PathBuf::from).collect()
    } else {
        env::split_paths(OsStr::new(raw)).collect()
    }
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;
    use neomax_core::providers::catalog::MapEnvironment;
    use serde_json::json;

    fn fixture_environment() -> MapEnvironment {
        MapEnvironment::new(std::iter::empty()).with_home("/fixture/home")
    }

    #[test]
    fn source_paths_remain_relocated_and_provider_capability_is_local() {
        let source = FilesystemPortalSource::new("/fixture/home", "/fixture/state");
        assert_eq!(source.paths().state, Path::new("/fixture/state"));
        let telemetry = json!({"available": true});
        let environment = fixture_environment();
        let capabilities = capabilities_for(
            Engine::Opencode,
            Path::new("/fixture/home"),
            &None,
            &telemetry,
            &environment,
        );
        assert!(!capabilities.binary_available);
        assert!(capabilities.usage_discovery);
        assert!(!capabilities.quota.supported);
        assert!(!capabilities.quota.available);
        assert!(capabilities.quota.source.is_none());
        assert!(capabilities.quota.windows.is_empty());
        assert!(capabilities.quota.reactive);
    }

    #[test]
    fn numeric_quota_is_supported_only_when_a_numeric_window_is_cached() {
        let telemetry = json!({"available": true});
        let environment = fixture_environment();
        let usage = Some(json!({
            "source": "claude-api",
            "five_hour": {"used_percent": 42.0},
            "seven_day": {"used_percent": 11.0}
        }));
        let capabilities = capabilities_for(
            Engine::Claude,
            Path::new("/fixture/home"),
            &usage,
            &telemetry,
            &environment,
        );
        assert!(capabilities.quota.supported);
        assert!(capabilities.quota.available);
        assert_eq!(capabilities.quota.source.as_deref(), Some("claude-api"));
        assert_eq!(capabilities.quota.windows, ["five_hour", "seven_day"]);
        assert!(!capabilities.quota.reactive);

        let unavailable = capabilities_for(
            Engine::Claude,
            Path::new("/fixture/home"),
            &None,
            &telemetry,
            &environment,
        );
        assert!(unavailable.quota.supported);
        assert!(!unavailable.quota.available);
        assert!(unavailable.quota.source.is_none());
        assert!(unavailable.quota.windows.is_empty());
        assert!(!unavailable.quota.reactive);
    }

    #[test]
    fn reactive_providers_keep_session_telemetry_separate_from_quota() {
        let telemetry = json!({
            "available": true,
            "source": "session-artifacts",
            "totals": {"out": 123}
        });
        let environment = fixture_environment();
        for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
            let capabilities = capabilities_for(
                engine,
                Path::new("/fixture/home"),
                &None,
                &telemetry,
                &environment,
            );
            assert!(capabilities.usage_discovery);
            assert!(!capabilities.quota.supported);
            assert!(!capabilities.quota.available);
            assert!(capabilities.quota.source.is_none());
            assert!(capabilities.quota.windows.is_empty());
            assert!(capabilities.quota.reactive);
        }
    }

    #[test]
    fn relative_binary_overrides_and_profile_roots_fail_closed() {
        let telemetry = json!({});
        let environment =
            MapEnvironment::new([("NEOMAX_OPENCODE_BIN".into(), "./opencode".into())])
                .with_home("relative/home")
                .with_current_dir("relative/workspace");
        let capabilities = capabilities_for(
            Engine::Opencode,
            Path::new("relative/home"),
            &None,
            &telemetry,
            &environment,
        );
        assert!(!capabilities.binary_available);
        assert!(capabilities.profile_root.as_os_str().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_binary_overrides_and_homes_fail_closed() {
        let telemetry = json!({});
        let environment = MapEnvironment::new([
            ("NEOMAX_OPENCODE_BIN".into(), r"C:tools\opencode.exe".into()),
            ("PATH".into(), r"C:tools;C:\Windows\System32".into()),
        ])
        .with_platform(neomax_core::runtime::RuntimePlatform::Windows)
        .with_home(r"C:relative-home")
        .with_current_dir(r"C:\workspace");
        let capabilities = capabilities_for(
            Engine::Opencode,
            Path::new(r"C:relative-home"),
            &None,
            &telemetry,
            &environment,
        );
        assert!(!capabilities.binary_available);
        assert!(capabilities.profile_root.as_os_str().is_empty());
    }
}
