use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use super::*;

#[derive(Debug, Clone, Copy)]
enum RenameFault {
    Pass,
    CrossDevice,
    PermissionDenied,
}

#[derive(Debug)]
struct FaultyRenamer {
    faults: Mutex<VecDeque<RenameFault>>,
}

impl FaultyRenamer {
    fn new(faults: impl IntoIterator<Item = RenameFault>) -> Self {
        Self {
            faults: Mutex::new(faults.into_iter().collect()),
        }
    }
}

impl RenameOps for FaultyRenamer {
    fn rename(&self, source: &Path, target: &Path) -> io::Result<()> {
        match self.faults.lock().expect("fault queue lock").pop_front() {
            Some(RenameFault::CrossDevice) => Err(cross_device_error()),
            Some(RenameFault::PermissionDenied) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected activation failure",
            )),
            Some(RenameFault::Pass) | None => system_rename(source, target),
        }
    }
}

fn cross_device_error() -> io::Error {
    #[cfg(unix)]
    {
        io::Error::from_raw_os_error(libc::EXDEV)
    }
    #[cfg(windows)]
    {
        io::Error::from_raw_os_error(17)
    }
    #[cfg(not(any(unix, windows)))]
    {
        io::Error::new(io::ErrorKind::Other, "injected cross-device failure")
    }
}

fn file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent");
    }
    fs::write(path, contents).expect("fixture file");
}

fn replacement(source: &Path, target: &Path) -> Replacement {
    Replacement {
        source: source.to_path_buf(),
        target: target.to_path_buf(),
    }
}

#[test]
fn cross_device_activation_copies_into_target_directory_and_preserves_mode() {
    let package = tempfile::tempdir().expect("package directory");
    let destination = tempfile::tempdir().expect("destination directory");
    let backup_parent = tempfile::tempdir().expect("backup directory");
    let source = package.path().join("neomax");
    let target = destination.path().join("bin/neomax");
    file(&source, b"new");
    file(&target, b"old");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o750)).expect("source mode");
    }

    let renamer = FaultyRenamer::new([
        RenameFault::Pass,
        RenameFault::CrossDevice,
        RenameFault::Pass,
    ]);
    replace_all_with(
        &[replacement(&source, &target)],
        backup_parent.path(),
        &renamer,
    )
    .expect("cross-device activation should succeed");

    assert_eq!(fs::read(&target).expect("activated file"), b"new");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&target)
                .expect("target metadata")
                .permissions()
                .mode()
                & 0o7777,
            0o750
        );
    }
}

#[test]
fn cross_device_backup_does_not_partial_activate_when_activation_fails() {
    let package = tempfile::tempdir().expect("package directory");
    let destination = tempfile::tempdir().expect("destination directory");
    let backup_parent = tempfile::tempdir().expect("backup directory");
    let source = package.path().join("new");
    let target = destination.path().join("home/.config/tool/settings.json");
    file(&source, b"new");
    file(&target, b"old");

    let renamer = FaultyRenamer::new([
        RenameFault::CrossDevice,
        RenameFault::Pass,
        RenameFault::PermissionDenied,
    ]);
    let error = replace_all_with(
        &[replacement(&source, &target)],
        backup_parent.path(),
        &renamer,
    )
    .expect_err("injected activation failure");

    assert!(
        error
            .to_string()
            .contains("could not activate installation file")
    );
    assert_eq!(fs::read(&target).expect("original target"), b"old");
    assert_eq!(fs::read(&source).expect("source remains staged"), b"new");
}

#[cfg(unix)]
#[test]
fn cross_device_copy_preserves_existing_symlink_without_following_it() {
    use std::os::unix::fs::symlink;

    let package = tempfile::tempdir().expect("package directory");
    let destination = tempfile::tempdir().expect("destination directory");
    let backup_parent = tempfile::tempdir().expect("backup directory");
    let outside = destination.path().join("outside");
    let source = package.path().join("new");
    let target = destination.path().join("alias");
    file(&outside, b"outside");
    file(&source, b"new");
    symlink("outside", &target).expect("target symlink");

    let renamer = FaultyRenamer::new([
        RenameFault::CrossDevice,
        RenameFault::Pass,
        RenameFault::CrossDevice,
        RenameFault::Pass,
    ]);
    replace_all_with(
        &[replacement(&source, &target)],
        backup_parent.path(),
        &renamer,
    )
    .expect("cross-device symlink replacement");

    assert_eq!(fs::read(&outside).expect("outside file"), b"outside");
    assert_eq!(fs::read(&target).expect("new target"), b"new");
    assert!(
        !fs::symlink_metadata(&target)
            .expect("target metadata")
            .file_type()
            .is_symlink()
    );
}

#[test]
fn directory_sources_and_targets_are_rejected_before_activation() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    fs::create_dir(&source).expect("source directory");
    fs::create_dir(&target).expect("target directory");

    let error = replace_all(&[replacement(&source, &target)], temp.path()).expect_err("directory");
    assert!(error.to_string().contains("regular file or symbolic link"));
    assert!(source.is_dir());
    assert!(target.is_dir());
}

#[test]
fn remove_all_uses_copy_fallback_and_restores_on_later_failure() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let backup_parent = tempfile::tempdir().expect("backup directory");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    file(&first, b"first");
    file(&second, b"second");

    let renamer = FaultyRenamer::new([
        RenameFault::CrossDevice,
        RenameFault::Pass,
        RenameFault::PermissionDenied,
    ]);
    let error = remove_all_with(
        &[first.clone(), second.clone()],
        backup_parent.path(),
        &renamer,
    )
    .expect_err("second removal should fail");

    assert!(
        error
            .to_string()
            .contains("could not stage installation file")
    );
    assert_eq!(fs::read(&first).expect("first restored"), b"first");
    assert_eq!(fs::read(&second).expect("second preserved"), b"second");
}

#[test]
fn duplicate_targets_are_rejected_without_touching_sources() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let target = temp.path().join("target");
    file(&first, b"first");
    file(&second, b"second");
    file(&target, b"old");

    let error = replace_all(
        &[replacement(&first, &target), replacement(&second, &target)],
        temp.path(),
    )
    .expect_err("duplicate target");
    assert!(error.to_string().contains("appears more than once"));
    assert_eq!(fs::read(&first).expect("first source"), b"first");
    assert_eq!(fs::read(&second).expect("second source"), b"second");
    assert_eq!(fs::read(&target).expect("target"), b"old");
}
