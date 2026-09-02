use super::fixture::codex_profile;
use crate::orchestration::auth::{FsCredentialWriter, codex};

#[test]
fn copy_and_swap_keep_the_complete_auth_json_blob() {
    let temp = tempfile::tempdir().unwrap();
    let first = codex_profile(temp.path(), ".codex-1", "first", 1);
    let second = codex_profile(temp.path(), ".codex-2", "second", 2);
    let copy = codex::prepare_copy(&FsCredentialWriter, &second, &first).unwrap();
    assert!(
        String::from_utf8(copy.mutations[0].bytes.clone().unwrap())
            .unwrap()
            .contains("\"extra\":1")
    );
    let swap = codex::prepare_swap(&FsCredentialWriter, &first, &second).unwrap();
    assert_eq!(swap.mutations.len(), 2);
    assert!(
        String::from_utf8(swap.mutations[0].bytes.clone().unwrap())
            .unwrap()
            .contains("\"extra\":2")
    );
}

#[test]
fn missing_auth_is_rejected_but_the_raw_auth_blob_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".codex-1");
    let destination = codex_profile(temp.path(), ".codex-2", "destination", 2);
    std::fs::create_dir_all(&source).unwrap();
    assert!(codex::prepare_copy(&FsCredentialWriter, &destination, &source).is_err());
    std::fs::write(codex::auth_path(&source), b"not-json").unwrap();
    let plan = codex::prepare_copy(&FsCredentialWriter, &destination, &source).unwrap();
    assert_eq!(
        plan.mutations[0].bytes.as_deref(),
        Some(b"not-json".as_slice())
    );
}
