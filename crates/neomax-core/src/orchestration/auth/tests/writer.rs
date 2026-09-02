use std::fs;

use crate::orchestration::auth::limits::MAX_CREDENTIAL_BYTES;
use crate::orchestration::auth::{CredentialWriter, FsCredentialWriter};

#[test]
fn oversized_credential_file_is_rejected_before_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join(".credentials.json");
    fs::write(&path, vec![b'x'; MAX_CREDENTIAL_BYTES + 1]).unwrap();
    assert!(FsCredentialWriter.read_optional(&path).is_err());
}

#[test]
fn missing_credential_file_remains_an_optional_absence() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        FsCredentialWriter
            .read_optional(&temp.path().join("missing.json"))
            .unwrap()
            .is_none()
    );
}
