use std::fs;
use std::path::{Path, PathBuf};

use crate::agent_tools::{ExecutableInputs, ExecutableSource, resolve_executable};

#[test]
fn current_executable_has_priority_over_install_bin() {
    let temp = tempfile::tempdir().unwrap();
    let current = candidate_path(temp.path(), "current-neomax");
    let installed = candidate_path(temp.path(), "installed-neomax");
    fs::write(&current, b"current").unwrap();
    fs::write(&installed, b"installed").unwrap();
    make_executable(&current);
    make_executable(&installed);

    let resolved =
        resolve_executable(&ExecutableInputs::new(Some(current), Some(installed))).unwrap();
    assert_eq!(resolved.source, ExecutableSource::CurrentExecutable);
}

#[test]
fn install_bin_is_used_when_current_executable_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let installed = candidate_path(temp.path(), "installed-neomax");
    fs::write(&installed, b"installed").unwrap();
    make_executable(&installed);

    let resolved = resolve_executable(&ExecutableInputs::new(
        Some(temp.path().join("missing")),
        Some(installed.clone()),
    ))
    .unwrap();
    assert_eq!(resolved.source, ExecutableSource::InstallBin);
    assert_eq!(resolved.path, fs::canonicalize(installed).unwrap());
}

#[test]
fn relative_candidates_are_not_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let installed = temp.path().join("installed-neomax");
    fs::write(&installed, b"installed").unwrap();
    make_executable(&installed);
    assert!(resolve_executable(&ExecutableInputs::new(Some("neomax".into()), None,)).is_err());
}

#[cfg(windows)]
#[test]
fn windows_requires_an_executable_extension() {
    let temp = tempfile::tempdir().unwrap();
    let candidate = temp.path().join("not-an-executable.txt");
    fs::write(&candidate, b"text").unwrap();
    assert!(resolve_executable(&ExecutableInputs::new(Some(candidate), None)).is_err());
}

fn candidate_path(root: &Path, stem: &str) -> PathBuf {
    #[cfg(windows)]
    {
        root.join(format!("{stem}.exe"))
    }
    #[cfg(not(windows))]
    {
        root.join(stem)
    }
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) {}
