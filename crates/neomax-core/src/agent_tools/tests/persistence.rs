#[cfg(unix)]
use std::fs;

use crate::agent_tools::{ManifestStore, ToolManifest};

#[test]
fn canonical_manifest_round_trips_through_private_atomic_store() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tools.json");
    let store = ManifestStore::new(&path);
    let written = store.write_canonical().unwrap();
    let read = store.read().unwrap();
    assert_eq!(written, read);
    assert_eq!(read, ToolManifest::canonical());
    assert_eq!(store.read_private_canonical().unwrap(), written);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(windows)]
    assert!(crate::io::verify_private_path(&path).is_ok());
}

#[cfg(unix)]
#[test]
fn private_manifest_reader_rejects_group_or_world_access() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tools.json");
    let store = ManifestStore::new(&path);
    store.write_canonical().unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store.read_private_canonical().is_err());
}

#[test]
fn invalid_manifest_is_not_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tools.json");
    let store = ManifestStore::new(&path);
    let mut manifest = ToolManifest::canonical();
    manifest.commands.clear();
    assert!(store.write(&manifest).is_err());
    assert!(!path.exists());
}
