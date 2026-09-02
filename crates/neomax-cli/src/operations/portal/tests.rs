use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tempfile::TempDir;

use super::*;

#[derive(Clone)]
struct RecordingExecutor {
    invocations: Arc<Mutex<Vec<PortalInvocation>>>,
    exit: PortalExit,
}

impl PortalExecutor for RecordingExecutor {
    fn invoke(&self, invocation: &PortalInvocation) -> Result<PortalExit> {
        self.invocations.lock().unwrap().push(invocation.clone());
        Ok(self.exit)
    }
}

fn fixture_directory() -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let current = directory.path().join("neomax");
    fs::write(&current, b"neomax").unwrap();
    let portal = directory.path().join(PORTAL_BINARY_FILENAME);
    fs::write(&portal, b"portal").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&portal).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&portal, permissions).unwrap();
    }
    (directory, current)
}

#[test]
fn resolves_only_the_regular_installed_sibling() {
    let (_directory, current) = fixture_directory();
    assert_eq!(
        sibling_executable(&current).unwrap(),
        current.parent().unwrap().join(PORTAL_BINARY_FILENAME)
    );
}

#[test]
fn refuses_missing_or_non_executable_sibling() {
    let (directory, current) = fixture_directory();
    let portal = directory.path().join(PORTAL_BINARY_FILENAME);
    fs::remove_file(&portal).unwrap();
    assert!(sibling_executable(&current).is_err());

    fs::write(&portal, b"portal").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&portal).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&portal, permissions).unwrap();
        assert!(sibling_executable(&current).is_err());
    }
}

#[test]
fn forwards_portal_arguments_without_shell_or_provider_resolution() {
    let (_directory, current) = fixture_directory();
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let executor = RecordingExecutor {
        invocations: Arc::clone(&invocations),
        exit: PortalExit::success(),
    };
    let args = vec![
        "8788".into(),
        "--home".into(),
        "/fixture/home".into(),
        "--state=/fixture/state".into(),
        "--days".into(),
        "7".into(),
    ];
    run_with_executor_without_environment(&current, &args, &executor, None).unwrap();
    let calls = invocations.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].args, args);
    assert_eq!(
        calls[0].executable,
        current.parent().unwrap().join(PORTAL_BINARY_FILENAME)
    );
}

#[test]
fn validated_override_wins_before_sibling_resolution() {
    let (directory, current) = fixture_directory();
    let sibling = directory.path().join(PORTAL_BINARY_FILENAME);
    fs::remove_file(sibling).unwrap();
    let override_path = directory.path().join("portal-override");
    fs::write(&override_path, b"portal").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&override_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&override_path, permissions).unwrap();
    }

    let invocations = Arc::new(Mutex::new(Vec::new()));
    let executor = RecordingExecutor {
        invocations: Arc::clone(&invocations),
        exit: PortalExit::success(),
    };
    run_with_executor_without_environment(&current, &[], &executor, Some(&override_path)).unwrap();
    assert_eq!(invocations.lock().unwrap()[0].executable, override_path);
}

#[test]
fn invalid_override_fails_closed_without_falling_back_to_sibling() {
    let (_directory, current) = fixture_directory();
    let invalid = current.parent().unwrap().join("missing-portal");
    let error = portal_executable_from_override(&current, Some(&invalid)).unwrap_err();
    assert!(error.to_string().contains(PORTAL_BINARY_ENV));

    let relative = Path::new("relative-portal");
    let error = portal_executable_from_override(&current, Some(relative)).unwrap_err();
    assert!(error.to_string().contains("absolute"));
}

#[test]
fn reports_child_exit_and_signal_without_replacing_the_invocation() {
    let (_directory, current) = fixture_directory();
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let failed = RecordingExecutor {
        invocations: Arc::clone(&invocations),
        exit: PortalExit {
            code: Some(17),
            signal: None,
        },
    };
    let error = run_with_executor_without_environment(&current, &[], &failed, None).unwrap_err();
    assert!(error.to_string().contains("status 17"));

    let signaled = RecordingExecutor {
        invocations,
        exit: PortalExit {
            code: None,
            signal: Some(2),
        },
    };
    let error = run_with_executor_without_environment(&current, &[], &signaled, None).unwrap_err();
    assert!(error.to_string().contains("signal 2"));
}
