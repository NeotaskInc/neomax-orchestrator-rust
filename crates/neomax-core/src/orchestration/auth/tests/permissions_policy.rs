use std::fs;

use crate::Engine;
use crate::orchestration::auth::permissions::{enforce_private_path, ensure_private_directory};
use crate::orchestration::auth::{copy_allowed, handoff_required};

#[test]
fn creates_private_directory_and_file() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("backups");
    ensure_private_directory(&directory).unwrap();
    let path = directory.join("credential.json");
    fs::write(&path, b"fixture").unwrap();
    enforce_private_path(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn only_claude_and_codex_allow_copy() {
    assert!(copy_allowed(Engine::Claude).is_ok());
    assert!(copy_allowed(Engine::Codex).is_ok());
    for engine in [Engine::Opencode, Engine::Kimi, Engine::Grok] {
        assert!(copy_allowed(engine).is_err());
        assert!(handoff_required(engine));
    }
}
