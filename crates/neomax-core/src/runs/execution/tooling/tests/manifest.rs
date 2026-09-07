use std::fs;

use crate::Engine;
use crate::agent_tools::{
    ExecutableInputs, ManifestStore, ToolManifest, canonical_manifest_relative_path,
};
use crate::runs::execution::tooling::{WorkerToolingInput, prepare_worker_tools};

use super::fixtures::{executable, paths, request, settings};

#[test]
fn first_worker_preparation_creates_the_private_canonical_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let legacy_path = paths.state.join(crate::agent_tools::MANIFEST_RELATIVE_PATH);
    fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    fs::write(
        &legacy_path,
        b"legacy definitions from a previous installation",
    )
    .unwrap();
    let request = request(Engine::Opencode, temp.path());
    let prepared = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    })
    .unwrap();

    let manifest_path = paths.state.join(canonical_manifest_relative_path());
    let manifest = ManifestStore::new(&manifest_path).read().unwrap();
    assert_eq!(manifest, ToolManifest::canonical());
    assert!(manifest.command("dispatch").is_some());
    assert!(prepared.variables().contains_key("NEOMAX_TOOL_MANIFEST"));
    assert_ne!(manifest_path, legacy_path);
    assert_eq!(
        fs::read(&legacy_path).unwrap(),
        b"legacy definitions from a previous installation"
    );
    assert!(!String::from_utf8_lossy(&fs::read(&manifest_path).unwrap()).contains("secret"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&manifest_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn noncanonical_manifest_is_rejected_without_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let binary = executable(temp.path(), "neomax");
    let paths = paths(temp.path());
    let manifest_path = paths.state.join(canonical_manifest_relative_path());
    fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
    let mut manifest = ToolManifest::canonical();
    manifest.commands[0].summary = "tampered".into();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    let request = request(Engine::Kimi, temp.path());
    let result = prepare_worker_tools(WorkerToolingInput {
        paths: &paths,
        settings: &settings(),
        request: &request,
        executable_inputs: ExecutableInputs::new(Some(binary), None),
        ambient_path: None,
        inherited_depth: None,
        inherited_max_depth: None,
    });
    assert!(result.is_err());
    assert_eq!(
        fs::read(&manifest_path).unwrap(),
        serde_json::to_vec(&manifest).unwrap()
    );
}
