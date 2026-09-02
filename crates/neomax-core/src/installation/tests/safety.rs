use std::fs;

use super::super::install::install;
use super::super::transaction::{Replacement, replace_all};
use super::super::types::{InstallOptions, UninstallOptions};
use super::super::uninstall::uninstall;
use super::support::{binary_path, fixture};

#[test]
fn force_does_not_replace_or_remove_directories_at_owned_paths() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();

    fs::remove_file(binary_path(&paths.bin_dir, "neomax")).unwrap();
    fs::create_dir(binary_path(&paths.bin_dir, "neomax")).unwrap();
    let error = install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: true,
    })
    .unwrap_err();
    assert!(error.to_string().contains("non-file installation path"));
    assert!(binary_path(&paths.bin_dir, "neomax").is_dir());

    fs::remove_dir(binary_path(&paths.bin_dir, "neomax")).unwrap();
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home),
        force: false,
    })
    .unwrap();
    fs::remove_file(binary_path(&paths.bin_dir, "neomax")).unwrap();
    fs::create_dir(binary_path(&paths.bin_dir, "neomax")).unwrap();
    let error = uninstall(UninstallOptions {
        paths: Some(paths.clone()),
        force: true,
    })
    .unwrap_err();
    assert!(error.to_string().contains("non-file installation path"));
    assert!(binary_path(&paths.bin_dir, "neomax").is_dir());
}

#[test]
fn bounded_manifest_reader_rejects_oversized_input() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("manifest.json");
    fs::write(&path, b"12345").unwrap();
    let error = super::super::files::read_bounded(&path, 4).unwrap_err();
    assert!(error.to_string().contains("4-byte limit"));
}

#[test]
fn failed_activation_restores_previous_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let missing_source = temp.path().join("missing-source");
    let second_target = temp.path().join("second-target");
    fs::write(&source, b"new").unwrap();
    fs::write(&target, b"old").unwrap();
    fs::write(&second_target, b"old-second").unwrap();

    let error = replace_all(
        &[
            Replacement {
                source,
                target: target.clone(),
            },
            Replacement {
                source: missing_source,
                target: second_target.clone(),
            },
        ],
        temp.path(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("could not activate installation file")
    );
    assert_eq!(fs::read(target).unwrap(), b"old");
    assert_eq!(fs::read(second_target).unwrap(), b"old-second");
}
