use std::fs;

use super::super::install::install;
use super::super::package::Package;
use super::super::paths::PackageRoot;
use super::super::types::{
    InstallOptions, ALIASES, ASSETS, AUXILIARIES, KIMI_AGENT_ASSET, SHELL_ASSETS,
};
use super::support::{binary_path, fixture};

#[test]
fn package_rejects_invalid_kimi_agent_asset() {
    let (package, _destination, _paths) = fixture();
    let asset = package.path().join("share/neomax").join(KIMI_AGENT_ASSET);
    fs::write(
        &asset,
        "---\nname: neomax\ndescription: Neomax orchestration agent\n---\nNo base prompt.\n",
    )
    .unwrap();
    let root = PackageRoot::new(package.path()).unwrap();
    let error = Package::load(&root).unwrap_err();
    assert!(error.to_string().contains("${base_prompt}"));
}

#[test]
fn install_materializes_all_commands_and_assets() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    let report = install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();
    assert!(!report.upgraded);
    for name in ALIASES {
        let path = paths.bin_dir.join(super::super::package::binary_name(name));
        assert!(super::super::files::path_exists(&path));
    }
    for name in AUXILIARIES {
        assert!(binary_path(&paths.bin_dir, name).is_file());
    }
    for name in ASSETS {
        assert!(paths.asset_path(name).is_file());
    }
    for name in SHELL_ASSETS {
        assert!(paths.asset_path(name).is_file());
    }
    assert!(home.join(".claude/commands/neomax.md").is_file());
    assert!(home.join(".claude/commands/project.md").is_file());
    assert!(home.join(".codex/prompts/neomax.md").is_file());
    assert!(home.join(".codex/prompts/project.md").is_file());
    assert!(home.join(".config/opencode/commands/neomax.md").is_file());
    assert!(home.join(".config/opencode/commands/project.md").is_file());
    assert!(home.join(".kimi-code/skills/neomax/SKILL.md").is_file());
    assert!(home.join(".kimi-code/skills/project/SKILL.md").is_file());
    assert!(home.join(".kimi-code/agents/neomax.md").is_file());
    assert!(!home.join(".kimi-code/SYSTEM.md").exists());
    assert!(home.join(".grok/commands/neomax.md").is_file());
    assert!(home.join(".grok/commands/project.md").is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(home.join(".claude/settings.json")).unwrap()).unwrap();
    assert!(settings["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["hooks"].as_array().into_iter().flatten())
        .any(|hook| hook["command"].as_str().unwrap().contains("orient --hook")));
    assert!(paths.manifest_path().is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.manifest_path()).unwrap()).unwrap();
    assert!(manifest["files"].as_array().unwrap().iter().any(|file| {
        file["path"].as_str().is_some_and(|path| {
            path.replace('\\', "/")
                .ends_with("share/neomax/workflows/project.md")
        })
    }));
    let workflow_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(paths.workflow_manifest_path()).unwrap()).unwrap();
    for suffix in [
        ".claude/commands/project.md",
        ".codex/prompts/project.md",
        ".config/opencode/commands/project.md",
        ".kimi-code/skills/project/SKILL.md",
        ".grok/commands/project.md",
    ] {
        assert!(
            workflow_manifest["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| {
                    file["path"]
                        .as_str()
                        .is_some_and(|path| path.replace('\\', "/").ends_with(suffix))
                }),
            "workflow manifest lacks project target {suffix}"
        );
    }
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(binary_path(&paths.bin_dir, "ocmax")).unwrap(),
        std::path::Path::new("neomax")
    );
}

#[test]
fn upgrades_owned_files_and_refuses_unrelated_files() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();
    fs::write(
        binary_path(package.path().join("bin"), "neomax"),
        b"main-v2",
    )
    .unwrap();
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();
    assert_eq!(
        fs::read(binary_path(&paths.bin_dir, "neomax")).unwrap(),
        b"main-v2"
    );
    fs::write(binary_path(&paths.bin_dir, "neomax"), b"user-edit").unwrap();
    let error = install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap_err();
    assert!(error.to_string().contains("modified installed file"));
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home),
        force: true,
    })
    .unwrap();
    assert_eq!(
        fs::read(binary_path(&paths.bin_dir, "neomax")).unwrap(),
        b"main-v2"
    );
}

#[test]
fn workflow_upgrade_refuses_modified_profile_file() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();
    let workflow = home.join(".claude/commands/rotate.md");
    fs::write(&workflow, b"user-owned edit").unwrap();
    let error = install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths),
        profile_home: Some(home),
        force: false,
    })
    .unwrap_err();
    assert!(error.to_string().contains("modified workflow"));
}
