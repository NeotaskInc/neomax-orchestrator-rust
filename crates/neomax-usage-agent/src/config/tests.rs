use super::AgentConfig;
use super::environment::ServiceEnvironment;
use super::paths::AgentPaths;
use super::validation::{validated_path, validated_provider_value};
use neomax_core::config::StatePaths;
use neomax_core::providers::catalog::all_specs;
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[test]
fn service_environment_records_portable_roots_and_absolute_binaries() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let state = StatePaths::new(&home, home.join(".neomax"));
    let paths = AgentPaths::for_state(state);
    let bin = home.join("bin");
    let executable = bin.join(if cfg!(windows) {
        "neomax-usage-agent.exe"
    } else {
        "neomax-usage-agent"
    });
    let neomax_cli = bin.join(if cfg!(windows) {
        "neomax.exe"
    } else {
        "neomax"
    });
    let environment = ServiceEnvironment::for_paths(&paths, &executable, &neomax_cli);
    assert_eq!(
        environment.values()["NEOMAX_USAGE_AGENT_BIN"],
        executable.display().to_string()
    );
    assert_eq!(
        environment.values()["NEOMAX_CLI_BIN"],
        neomax_cli.display().to_string()
    );
    assert_eq!(environment.values()["HOME"], home.display().to_string());
    assert_eq!(
        environment.values()["XDG_CONFIG_HOME"],
        home.join(".config").display().to_string()
    );
    assert_eq!(
        environment.values()["APPDATA"],
        home.join("AppData").join("Roaming").display().to_string()
    );
}

#[test]
fn configured_provider_roots_reject_relative_paths() {
    let temp = tempfile::tempdir().unwrap();
    for (index, provider) in all_specs().enumerate() {
        for name in [&provider.config_env, &provider.orchestrator_env] {
            let absolute = temp
                .path()
                .join(format!("provider-{index}-{name}"))
                .into_os_string();
            assert!(
                validated_provider_value(name, &absolute).is_ok(),
                "absolute provider root rejected for {name}"
            );
            let error =
                validated_provider_value(name, &OsString::from("relative/provider")).unwrap_err();
            assert!(
                error.to_string().contains("absolute path"),
                "relative provider root accepted for {name}"
            );
        }

        let absolute_profiles = [
            temp.path().join(format!("profile-{index}-one")),
            temp.path().join(format!("profile-{index}-two")),
        ];
        let joined = env::join_paths(&absolute_profiles).unwrap();
        assert!(
            validated_provider_value(&provider.profile_env, &joined).is_ok(),
            "absolute profile list rejected for {}",
            provider.profile_env
        );
        let invalid = env::join_paths([
            absolute_profiles[0].clone(),
            PathBuf::from("relative/profile"),
        ])
        .unwrap();
        let error = validated_provider_value(&provider.profile_env, &invalid).unwrap_err();
        assert!(
            error.to_string().contains("only absolute paths"),
            "relative profile accepted for {}",
            provider.profile_env
        );
    }
}

#[test]
fn fixture_config_uses_platform_executable_names() {
    let temp = tempfile::tempdir().unwrap();
    let state = StatePaths::new(temp.path(), temp.path().join(".neomax"));
    let config = AgentConfig::with_paths(AgentPaths::for_state(state));
    let expected_agent = if cfg!(windows) {
        "neomax-usage-agent.exe"
    } else {
        "neomax-usage-agent"
    };
    let expected_cli = if cfg!(windows) {
        "neomax.exe"
    } else {
        "neomax"
    };
    assert_eq!(
        config.executable,
        temp.path().join("bin").join(expected_agent)
    );
    assert_eq!(
        config.neomax_cli,
        temp.path().join("bin").join(expected_cli)
    );
}

#[test]
fn path_validation_rejects_relative_entries_and_anchors() {
    let temp = tempfile::tempdir().unwrap();
    assert!(validated_path(Some(OsStr::new("relative/bin")), &[temp.path()]).is_err());
    assert!(validated_path(None, &[std::path::Path::new("relative/bin/tool")]).is_err());
}

#[test]
fn discovered_agent_paths_require_absolute_service_roots() {
    let paths = AgentPaths::for_state_with_roots(
        StatePaths::new("relative/home", "relative/state"),
        PathBuf::from("relative/config"),
        PathBuf::from("relative/appdata"),
    );
    assert!(paths.validate().is_err());
}

#[cfg(windows)]
#[test]
fn windows_partial_roots_are_rejected_by_environment_validation() {
    use super::validation::{absolute_env_path, require_absolute};
    use std::path::Path;

    assert!(require_absolute("fixture", Path::new(r"C:relative")).is_err());
    assert!(require_absolute("fixture", Path::new(r"\relative")).is_err());
    assert!(absolute_env_path("NEOMAX_MISSING_FIXTURE", PathBuf::from(r"C:relative")).is_err());
}
