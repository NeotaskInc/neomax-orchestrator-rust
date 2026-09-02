use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::agent_tools::{
    EnvironmentInput, LaunchRole, NEOMAX_BIN_ENV, NEOMAX_TOOL_DEPTH_ENV,
    NEOMAX_TOOL_INSTRUCTION_ENV, NEOMAX_TOOL_MANIFEST_ENV, NEOMAX_TOOL_MAX_DEPTH_ENV,
    RecursionGuard, augment_path, build_environment,
};

fn fixture_paths() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf, OsString) {
    let temp = tempfile::tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    let executable = bin_dir.join(executable_name("neomax"));
    let manifest = temp.path().join("state").join("tools.json");
    let install_bin = bin_dir.join(executable_name("neomax-install"));
    let existing_path = std::env::join_paths([
        temp.path().join("existing-one"),
        temp.path().join("existing-two"),
    ])
    .unwrap();
    (temp, executable, manifest, install_bin, existing_path)
}

fn executable_name(stem: &str) -> String {
    #[cfg(windows)]
    {
        format!("{stem}.exe")
    }
    #[cfg(not(windows))]
    {
        stem.into()
    }
}

#[test]
fn environment_contract_preserves_and_augments_path() {
    let (_temp, executable, manifest, install_bin, existing_path) = fixture_paths();
    let executable_value = executable.to_string_lossy().into_owned();
    let manifest_value = manifest.to_string_lossy().into_owned();
    let mut expected_path = vec![executable.parent().unwrap().to_path_buf()];
    expected_path.extend(std::env::split_paths(existing_path.as_os_str()));
    let environment = build_environment(EnvironmentInput {
        executable: &executable,
        manifest_path: &manifest,
        install_bin: Some(&install_bin),
        existing_path: Some(existing_path.as_os_str()),
        guard: RecursionGuard::new(0, 4).unwrap(),
        role: LaunchRole::Worker,
    })
    .unwrap();
    let variables = environment.variables();
    assert_eq!(variables.get(NEOMAX_BIN_ENV), Some(&executable_value));
    assert_eq!(variables.get(NEOMAX_TOOL_MANIFEST_ENV), Some(&manifest_value));
    assert_eq!(variables.get(NEOMAX_TOOL_DEPTH_ENV).unwrap(), "1");
    assert_eq!(variables.get(NEOMAX_TOOL_MAX_DEPTH_ENV).unwrap(), "4");
    assert_eq!(
        variables.get(NEOMAX_TOOL_INSTRUCTION_ENV).unwrap(),
        environment.instruction()
    );
    assert_eq!(
        std::env::split_paths(OsStr::new(variables.get("PATH").unwrap()))
            .collect::<Vec<_>>(),
        expected_path
    );
}

#[test]
fn every_supported_host_uses_the_same_environment_contract() {
    let (_temp, executable, manifest, _install_bin, _existing_path) = fixture_paths();
    let hosts = crate::agent_tools::OrchestratorHost::ALL;
    assert_eq!(hosts.len(), 5);
    assert_eq!(
        hosts.map(crate::agent_tools::OrchestratorHost::as_str),
        ["claude", "codex", "opencode", "kimi", "grok"]
    );
    for _host in hosts {
        let environment = build_environment(EnvironmentInput {
            executable: &executable,
            manifest_path: &manifest,
            install_bin: None,
            existing_path: None,
            guard: RecursionGuard::root(),
            role: LaunchRole::Worker,
        })
        .unwrap();
        assert!(environment.variables().contains_key(NEOMAX_BIN_ENV));
        assert!(
            environment
                .variables()
                .contains_key(NEOMAX_TOOL_MANIFEST_ENV)
        );
    }
}

#[test]
fn orchestrator_role_gets_root_depth_and_dispatch_instruction() {
    let (_temp, executable, manifest, _install_bin, _existing_path) = fixture_paths();
    let environment = build_environment(EnvironmentInput {
        executable: &executable,
        manifest_path: &manifest,
        install_bin: None,
        existing_path: None,
        guard: RecursionGuard::new(2, 4).unwrap(),
        role: LaunchRole::Orchestrator,
    })
    .unwrap();
    assert_eq!(
        environment.variables().get(NEOMAX_TOOL_DEPTH_ENV),
        Some(&"2".into())
    );
    assert!(environment.instruction().contains("Dispatch"));
    assert!(
        !environment
            .instruction()
            .contains("do not start another worker")
    );
}

#[test]
fn relative_executable_directories_are_rejected() {
    assert!(
        build_environment(EnvironmentInput {
            executable: Path::new("bin/neomax"),
            manifest_path: Path::new("tools.json"),
            install_bin: None,
            existing_path: None,
            guard: RecursionGuard::root(),
            role: LaunchRole::Worker,
        })
        .is_err()
    );
}

#[test]
fn inherited_path_keeps_only_absolute_entries() {
    let temp = tempfile::tempdir().unwrap();
    let absolute = temp.path().join("existing");
    let existing = std::env::join_paths([PathBuf::from("relative-bin"), absolute.clone()]).unwrap();
    let executable = temp.path().join("bin").join(executable_name("neomax"));

    let augmented = augment_path(Some(&existing), &executable, None).unwrap();
    let entries = std::env::split_paths(&augmented).collect::<Vec<_>>();

    assert!(entries.contains(&executable.parent().unwrap().to_path_buf()));
    assert!(entries.contains(&absolute));
    assert!(!entries.contains(&PathBuf::from("relative-bin")));
}

#[cfg(windows)]
#[test]
fn inherited_path_rejects_windows_partial_roots() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("bin").join(executable_name("neomax"));
    let existing = std::env::join_paths([
        PathBuf::from(r"\rooted"),
        PathBuf::from(r"C:drive-relative"),
        temp.path().join("safe"),
    ])
    .unwrap();

    let augmented = augment_path(Some(&existing), &executable, None).unwrap();
    let entries = std::env::split_paths(&augmented).collect::<Vec<_>>();

    assert!(!entries.contains(&PathBuf::from(r"\rooted")));
    assert!(!entries.contains(&PathBuf::from(r"C:drive-relative")));
}

#[cfg(unix)]
#[test]
fn non_utf8_security_paths_are_rejected_before_environment_serialization() {
    use std::os::unix::ffi::OsStringExt;

    let executable =
        std::path::PathBuf::from(OsString::from_vec(b"/tmp/neomax-\xff/bin/neomax".to_vec()));
    let error = build_environment(EnvironmentInput {
        executable: &executable,
        manifest_path: Path::new("/private/neomax/tools.json"),
        install_bin: None,
        existing_path: None,
        guard: RecursionGuard::root(),
        role: LaunchRole::Worker,
    })
    .unwrap_err();
    assert!(error.to_string().contains("Neomax executable path"));
    assert!(error.to_string().contains("UTF-8"));
}
