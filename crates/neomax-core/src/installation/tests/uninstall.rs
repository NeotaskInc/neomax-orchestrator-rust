use std::fs;

use super::super::install::install;
use super::super::types::{InstallOptions, UninstallOptions};
use super::super::uninstall::uninstall;
use super::support::{binary_path, fixture};

#[test]
fn uninstall_is_scoped_and_refuses_modified_files() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home),
        force: false,
    })
    .unwrap();
    fs::write(binary_path(&paths.bin_dir, "neomax-portal"), b"user-edit").unwrap();
    let error = uninstall(UninstallOptions {
        paths: Some(paths.clone()),
        force: false,
    })
    .unwrap_err();
    assert!(error.to_string().contains("modified installed file"));
    assert!(binary_path(&paths.bin_dir, "neomax").is_file());
    let report = uninstall(UninstallOptions {
        paths: Some(paths.clone()),
        force: true,
    })
    .unwrap();
    assert!(!report.removed.is_empty());
    assert!(!paths.manifest_path().exists());
    assert!(!binary_path(&paths.bin_dir, "neomax").exists());
    assert!(!binary_path(&paths.bin_dir, "neomax-portal").exists());
}

#[test]
fn uninstall_removes_owned_hooks_but_preserves_unrelated_settings() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    let settings_path = home.join(".claude/settings.json");
    fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
    fs::write(
        &settings_path,
        br#"{"hooks":{"SessionStart":[{"matcher":"project:*","hooks":[{"type":"command","command":"user-command"}]}]}}"#,
    )
    .unwrap();
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home),
        force: false,
    })
    .unwrap();
    uninstall(UninstallOptions {
        paths: Some(paths),
        force: false,
    })
    .unwrap();
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(settings_path).unwrap()).unwrap();
    assert_eq!(settings["hooks"]["SessionStart"][0]["matcher"], "project:*");
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "user-command"
    );
}
