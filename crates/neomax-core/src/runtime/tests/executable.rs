use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;

use crate::runtime::{
    quote_cmd_argument, resolve_provider_executable, RuntimeEnvironment, RuntimePlatform,
};

fn command_shell_fixture(root: &Path) -> std::path::PathBuf {
    fs::create_dir_all(root.join("System32")).unwrap();
    let root = root.canonicalize().unwrap();
    let path = root.join("System32").join("cmd.exe");
    fs::write(&path, b"fixture shell\n").unwrap();
    path
}

#[test]
fn cmd_resolution_uses_fixed_comspec_and_quotes_metacharacters() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("provider & space.cmd");
    fs::write(&shim, b"@echo off\r\n").unwrap();
    let path = temp.path().to_string_lossy().into_owned();
    let comspec = command_shell_fixture(&temp.path().join("windows"));
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("PATH".into(), path),
            ("ComSpec".into(), comspec.to_string_lossy().into_owned()),
        ],
        temp.path(),
    );
    let resolved = environment
        .resolve_provider_executable("provider & space")
        .unwrap();
    assert!(resolved.uses_command_shell);
    assert_eq!(resolved.program, comspec.as_os_str());
    let (_, args) = resolved
        .apply_to_command(&[OsString::from("--version")])
        .unwrap();
    assert_eq!(
        args[..4],
        [
            OsString::from("/d"),
            OsString::from("/e:on"),
            OsString::from("/v:off"),
            OsString::from("/c")
        ]
    );
    let command = args[4].to_string_lossy();
    assert!(command.contains("provider & space.cmd"));
    assert!(command.starts_with('"'));
    let (_, args) = resolved
        .apply_to_command(&[OsString::from("100% & ^ | < > (x) !")])
        .unwrap();
    let command = args[4].to_string_lossy();
    assert!(command.contains("100%%cd:~,%"));
    assert!(command.contains("& ^ | < > (x) !"));
}

#[test]
fn batch_resolution_uses_the_same_safe_comspec_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("provider.bat");
    fs::write(&shim, b"@echo off\r\n").unwrap();
    let comspec = command_shell_fixture(&temp.path().join("windows"));
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("PATH".into(), String::new()),
            ("ComSpec".into(), comspec.to_string_lossy().into_owned()),
        ],
        temp.path(),
    );
    let (_, args) = environment
        .resolve_provider_command_at(
            OsStr::new("provider"),
            &[OsString::from("100% & unsafe")],
            temp.path(),
        )
        .unwrap();
    assert!(args[4].to_string_lossy().contains("100%%cd:~,%"));
}

#[test]
fn cmd_argument_renderer_quotes_windows_metacharacter_paths() {
    let rendered = quote_cmd_argument(OsStr::new(
        r#"C:\Program Files (x86)\Neomax & Tools\worker\"#,
    ))
    .unwrap();
    assert_eq!(
        rendered,
        r#""C:\Program Files (x86)\Neomax & Tools\worker\\""#
    );
}

#[test]
fn cmd_argument_renderer_preserves_multiline_prompts_without_shell_line_breaks() {
    let rendered = quote_cmd_argument(OsStr::new("first\r\nsecond\nthird\rfourth")).unwrap();
    assert_eq!(rendered, "first\u{2028}second\u{2028}third\u{2028}fourth");
    assert!(!rendered.contains(['\r', '\n']));
}

#[test]
fn cmd_argument_renderer_rejects_nul() {
    assert!(quote_cmd_argument(OsStr::new("before\0after")).is_err());
}

#[cfg(unix)]
#[test]
fn cmd_argument_renderer_rejects_non_unicode_values() {
    use std::os::unix::ffi::OsStrExt;

    let value = OsStr::from_bytes(b"worker-\xff.cmd");
    assert!(quote_cmd_argument(value).is_err());
}

#[test]
fn malicious_comspec_falls_back_to_the_validated_system_root_shell() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("provider.cmd");
    fs::write(&shim, b"@echo off\r\n").unwrap();
    let root = temp.path().join("system-root");
    let fallback = command_shell_fixture(&root);
    let root = fallback.parent().unwrap().parent().unwrap().to_owned();
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("PATH".into(), temp.path().to_string_lossy().into_owned()),
            ("ComSpec".into(), "relative\\cmd.exe\nwhoami".into()),
            ("SystemRoot".into(), root.to_string_lossy().into_owned()),
        ],
        temp.path(),
    );
    let resolved = environment.resolve_provider_executable("provider").unwrap();
    assert_eq!(resolved.program, fallback.as_os_str());
    assert!(resolved.uses_command_shell);
}

#[cfg(unix)]
#[test]
fn symlink_comspec_falls_back_instead_of_becoming_the_shell() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("system-root");
    let fallback = command_shell_fixture(&root);
    let root = fallback.parent().unwrap().parent().unwrap().to_owned();
    let link = temp.path().join("linked-cmd.exe");
    std::os::unix::fs::symlink(&fallback, &link).unwrap();
    let script = temp.path().join("provider.bat");
    fs::write(&script, b"@echo off\r\n").unwrap();
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("PATH".into(), temp.path().to_string_lossy().into_owned()),
            ("ComSpec".into(), link.to_string_lossy().into_owned()),
            ("SystemRoot".into(), root.to_string_lossy().into_owned()),
        ],
        temp.path(),
    );
    let resolved = environment.resolve_provider_executable("provider").unwrap();
    assert_eq!(resolved.program, fallback.as_os_str());
}

#[test]
fn invalid_comspec_and_system_root_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("provider.cmd");
    fs::write(&script, b"@echo off\r\n").unwrap();
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("PATH".into(), temp.path().to_string_lossy().into_owned()),
            ("ComSpec".into(), "cmd.exe".into()),
            (
                "SystemRoot".into(),
                temp.path().join("missing").to_string_lossy().into_owned(),
            ),
        ],
        temp.path(),
    );
    assert!(environment.resolve_provider_executable("provider").is_err());
}

#[cfg(windows)]
#[test]
fn linked_component_metadata_errors_fail_closed() {
    let path = std::env::temp_dir().join(format!(
        "neomax-missing-runtime-component-{}",
        std::process::id()
    ));
    assert!(super::super::executable::has_linked_component(&path).is_err());
}

#[test]
fn executable_resolution_keeps_exe_direct() {
    let temp = tempfile::tempdir().unwrap();
    let shim = temp.path().join("provider.exe");
    fs::write(&shim, b"fixture").unwrap();
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [("PATH".into(), temp.path().to_string_lossy().into_owned())],
        temp.path(),
    );
    let resolved = environment.resolve_provider_executable("provider").unwrap();
    assert_eq!(resolved.program, shim.as_os_str());
    assert!(!resolved.uses_command_shell);
    assert!(resolved.prefix_args.is_empty());
}

#[test]
fn unix_resolution_keeps_program_and_arguments_unmodified() {
    let resolved = resolve_provider_executable(
        OsStr::new("provider with spaces"),
        RuntimePlatform::Unix,
        None,
        None,
        None,
        None,
        Path::new("/work"),
    )
    .unwrap();
    assert_eq!(resolved.program, OsString::from("provider with spaces"));
    assert!(!resolved.uses_command_shell);
    assert!(resolved.prefix_args.is_empty());
}

#[cfg(windows)]
#[test]
fn windows_partial_root_programs_and_current_directories_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    for program in [r"\rooted-provider.cmd", r"C:drive-relative.cmd"] {
        let error = resolve_provider_executable(
            OsStr::new(program),
            RuntimePlatform::Windows,
            None,
            None,
            None,
            None,
            temp.path(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("rooted but not absolute"));
    }
    for current_dir in [
        Path::new(r"\rooted-workdir"),
        Path::new(r"C:drive-relative"),
    ] {
        let error = resolve_provider_executable(
            OsStr::new("provider.exe"),
            RuntimePlatform::Windows,
            None,
            None,
            None,
            None,
            current_dir,
        )
        .unwrap_err();
        assert!(error.to_string().contains("must be absolute"));
    }
}

#[test]
fn windows_relative_current_directory_fails_closed() {
    let error = resolve_provider_executable(
        OsStr::new("provider.exe"),
        RuntimePlatform::Windows,
        None,
        None,
        None,
        None,
        Path::new("relative-workdir"),
    )
    .unwrap_err();

    assert!(error.to_string().contains("must be absolute"));
}

#[cfg(windows)]
#[test]
fn windows_partial_root_command_shell_paths_fail_closed() {
    for value in [r"\cmd.exe", r"C:cmd.exe"] {
        assert!(
            super::super::executable::resolve_command_shell(Some(OsStr::new(value)), None).is_err()
        );
    }
    for value in [r"\Windows", r"C:Windows"] {
        assert!(
            super::super::executable::resolve_command_shell(None, Some(OsStr::new(value))).is_err()
        );
    }
}
