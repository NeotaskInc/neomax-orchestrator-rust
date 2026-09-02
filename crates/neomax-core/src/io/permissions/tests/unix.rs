use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;

use super::super::{
    ensure_private_directory, set_private_open_path, set_private_path, verify_private_path,
};

#[test]
fn private_paths_use_owner_only_modes() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("private");
    ensure_private_directory(&directory).unwrap();
    let file = directory.join("secret.json");
    fs::write(&file, b"fixture").unwrap();
    set_private_path(&file).unwrap();
    verify_private_path(&directory).unwrap();
    verify_private_path(&file).unwrap();
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn private_open_paths_use_the_open_descriptor() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("private");
    ensure_private_directory(&directory).unwrap();
    let path = directory.join("secret.json");
    let file = File::create(&path).unwrap();
    set_private_open_path(&file, &path).unwrap();
    verify_private_path(&path).unwrap();
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn private_paths_reject_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    fs::write(&target, b"fixture").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(set_private_path(&link).is_err());
    assert!(verify_private_path(&link).is_err());
}
