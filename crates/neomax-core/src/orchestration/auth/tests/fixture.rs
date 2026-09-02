use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::orchestration::auth::{CredentialWriter, FsCredentialWriter, claude, codex};
use crate::{Error, Result};

pub fn claude_profile(root: &Path, name: &str, token: &str, uuid: &str) -> PathBuf {
    let profile = root.join(name);
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        claude::credential_path(&profile),
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{token}","refreshToken":"refresh"}},"extra":{{"preserve":true}}}}"#),
    )
    .unwrap();
    fs::write(
        claude::identity_path(&profile),
        format!(r#"{{"oauthAccount":{{"accountUuid":"{uuid}","emailAddress":"{uuid}@example.test"}},"settings":{{"keep":true}}}}"#),
    )
    .unwrap();
    profile
}

pub fn codex_profile(root: &Path, name: &str, token: &str, extra: u64) -> PathBuf {
    let profile = root.join(name);
    fs::create_dir_all(&profile).unwrap();
    fs::write(
        codex::auth_path(&profile),
        format!(r#"{{"tokens":{{"access_token":"{token}"}},"extra":{extra}}}"#),
    )
    .unwrap();
    profile
}

#[derive(Clone)]
pub struct FailOnceWriter {
    pub fail_path: PathBuf,
    failed: Arc<Mutex<bool>>,
}

impl FailOnceWriter {
    pub fn new(fail_path: PathBuf) -> Self {
        Self {
            fail_path,
            failed: Arc::new(Mutex::new(false)),
        }
    }
}

impl CredentialWriter for FailOnceWriter {
    fn read_optional(&self, path: &Path) -> Result<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let should_fail = path == self.fail_path && !*self.failed.lock().unwrap();
        if should_fail {
            *self.failed.lock().unwrap() = true;
            return Err(Error::Message("injected writer failure".into()));
        }
        FsCredentialWriter.write_atomic(path, bytes)
    }

    fn remove(&self, path: &Path) -> Result<()> {
        FsCredentialWriter.remove(path)
    }
}
