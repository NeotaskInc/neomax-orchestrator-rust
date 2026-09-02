use crate::agent_tools::{CommandClass, ToolManifest};

#[test]
fn canonical_manifest_is_stable_and_complete() {
    let first = ToolManifest::canonical();
    let second = ToolManifest::canonical();
    assert_eq!(first, second);
    assert!(first.is_canonical());
    first.validate().unwrap();
    assert!(first.command("dispatch").is_some());
    assert_eq!(
        first.command("config show").unwrap().class,
        CommandClass::ReadOnly
    );
    for command in [
        "portal",
        "select",
        "why",
        "projects",
        "project-register",
        "orient",
        "install",
        "uninstall",
    ] {
        assert!(first.command(command).is_some(), "missing {command}");
    }
}

#[test]
fn manifest_serialization_is_deterministic() {
    let manifest = ToolManifest::canonical();
    assert_eq!(
        manifest.json_bytes().unwrap(),
        manifest.json_bytes().unwrap()
    );
    let json = String::from_utf8(manifest.json_bytes().unwrap()).unwrap();
    assert!(json.ends_with('\n'));
}

#[test]
fn manifest_rejects_unknown_commands() {
    let mut manifest = ToolManifest::canonical();
    manifest.commands[0].command = "unknown".into();
    assert!(manifest.validate().is_err());
}

#[test]
fn manifest_rejects_tampered_command_policy() {
    let mut manifest = ToolManifest::canonical();
    let command = manifest.command("kill").unwrap().command.clone();
    let entry = manifest
        .commands
        .iter_mut()
        .find(|entry| entry.command == command)
        .unwrap();
    entry.class = CommandClass::ReadOnly;
    assert!(manifest.validate().is_err());
}

#[test]
fn manifest_rejects_an_incomplete_command_surface() {
    let mut manifest = ToolManifest::canonical();
    manifest.commands.retain(|entry| entry.command != "rotate");
    let error = manifest.validate().unwrap_err();
    assert!(error.to_string().contains("rotate"));
}
