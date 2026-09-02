use std::path::Path;

use crate::{Error, Result};

use super::files::{read_bounded, sha256, validate_relative};
use super::paths::InstallPaths;
use super::types::{
    InstallManifest, ManifestFile, ManifestKind, ALIASES, ASSETS, AUXILIARIES, DOCS,
    INSTALL_SCHEMA_VERSION, KIMI_AGENT_ASSET, PRODUCT, SHELL_ASSETS, WORKFLOWS,
};

impl InstallManifest {
    pub(crate) fn new(version: String, paths: &InstallPaths) -> Result<Self> {
        let mut files = Vec::new();
        for alias in ALIASES {
            let name = super::package::binary_name(alias);
            let path = paths.bin_dir.join(&name);
            let is_copy = *alias == "neomax" || cfg!(windows);
            let kind = if is_copy {
                ManifestKind::File
            } else {
                ManifestKind::Symlink
            };
            files.push(ManifestFile {
                path: format!("bin/{name}"),
                kind,
                sha256: if is_copy { Some(sha256(&path)?) } else { None },
                target: if !is_copy {
                    Some("neomax".into())
                } else {
                    None
                },
            });
        }
        for auxiliary in AUXILIARIES {
            let name = super::package::binary_name(auxiliary);
            let path = paths.bin_dir.join(&name);
            files.push(ManifestFile {
                path: format!("bin/{name}"),
                kind: ManifestKind::File,
                sha256: Some(sha256(&path)?),
                target: None,
            });
        }
        for asset in ASSETS {
            let path = paths.asset_path(asset);
            files.push(ManifestFile {
                path: format!("share/neomax/{asset}"),
                kind: ManifestKind::File,
                sha256: Some(sha256(&path)?),
                target: None,
            });
        }
        for asset in SHELL_ASSETS {
            let path = paths.asset_path(asset);
            files.push(ManifestFile {
                path: format!("share/neomax/{asset}"),
                kind: ManifestKind::File,
                sha256: Some(sha256(&path)?),
                target: None,
            });
        }
        for doc in DOCS {
            let path = paths.asset_path(doc);
            files.push(ManifestFile {
                path: format!("share/neomax/{doc}"),
                kind: ManifestKind::File,
                sha256: Some(sha256(&path)?),
                target: None,
            });
        }
        for workflow in WORKFLOWS {
            let path = paths.workflow_path(workflow);
            files.push(ManifestFile {
                path: format!("share/neomax/workflows/{workflow}"),
                kind: ManifestKind::File,
                sha256: Some(sha256(&path)?),
                target: None,
            });
        }
        let kimi_agent = paths.asset_path(KIMI_AGENT_ASSET);
        files.push(ManifestFile {
            path: format!("share/neomax/{KIMI_AGENT_ASSET}"),
            kind: ManifestKind::File,
            sha256: Some(sha256(&kimi_agent)?),
            target: None,
        });
        Ok(Self {
            schema_version: INSTALL_SCHEMA_VERSION,
            product: PRODUCT.into(),
            version,
            files,
        })
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.schema_version != INSTALL_SCHEMA_VERSION || self.product != PRODUCT {
            return Err(Error::InvalidArgument(
                "unsupported Neomax installation manifest".into(),
            ));
        }
        if self.version.is_empty() {
            return Err(Error::InvalidArgument(
                "installation manifest version is empty".into(),
            ));
        }
        let mut expected = ALIASES
            .iter()
            .chain(AUXILIARIES.iter())
            .map(|name| format!("bin/{}", super::package::binary_name(name)))
            .chain(ASSETS.iter().map(|name| format!("share/neomax/{name}")))
            .chain(SHELL_ASSETS.iter().map(|name| format!("share/neomax/{name}")))
            .chain(DOCS.iter().map(|name| format!("share/neomax/{name}")))
            .chain(
                WORKFLOWS
                    .iter()
                    .map(|name| format!("share/neomax/workflows/{name}")),
            )
            .chain(std::iter::once(format!("share/neomax/{KIMI_AGENT_ASSET}")))
            .collect::<Vec<_>>();
        expected.sort();
        let mut actual = self
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        actual.sort();
        if actual != expected {
            return Err(Error::InvalidArgument(
                "installation manifest does not describe the complete Neomax command surface"
                    .into(),
            ));
        }
        for file in &self.files {
            validate_relative(&file.path)?;
            if !file.path.starts_with("bin/") && !file.path.starts_with("share/neomax/") {
                return Err(Error::InvalidArgument(format!(
                    "installation manifest owns an unexpected path: {}",
                    file.path
                )));
            }
            let expected_kind = expected_kind(&file.path).ok_or_else(|| {
                Error::InvalidArgument(format!(
                    "installation manifest owns an unexpected path: {}",
                    file.path
                ))
            })?;
            if file.kind != expected_kind {
                return Err(Error::InvalidArgument(format!(
                    "installation manifest has an unexpected file kind: {}",
                    file.path
                )));
            }
            match &file.kind {
                ManifestKind::File => {
                    let valid_hash = file.sha256.as_deref().is_some_and(|hash| {
                        hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                    });
                    if !valid_hash || file.target.is_some() {
                        return Err(Error::InvalidArgument(format!(
                            "installation manifest has an invalid file record: {}",
                            file.path
                        )));
                    }
                }
                ManifestKind::Symlink => {
                    if file.target.as_deref() != Some("neomax") || file.sha256.is_some() {
                        return Err(Error::InvalidArgument(format!(
                            "installation manifest has an invalid alias record: {}",
                            file.path
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn read(paths: &InstallPaths) -> Result<Option<Self>> {
        let path = paths.manifest_path();
        if !path.is_file() {
            return Ok(None);
        }
        if !std::fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(Error::InvalidState {
                path,
                message: "installation manifest must be a regular file".into(),
            });
        }
        crate::io::verify_private_path(&path)?;
        let value = serde_json::from_slice::<Self>(&read_bounded(&path, 2 * 1024 * 1024)?)
            .map_err(|error| Error::InvalidState {
                path: path.clone(),
                message: error.to_string(),
            })?;
        value.validate()?;
        Ok(Some(value))
    }

    pub(crate) fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut data = serde_json::to_vec_pretty(self)?;
        data.push(b'\n');
        crate::atomic::write_bytes_atomic(path, &data)
    }

    pub(crate) fn file(&self, path: &str) -> Option<&ManifestFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

fn expected_kind(path: &str) -> Option<ManifestKind> {
    if let Some(name) = path.strip_prefix("bin/") {
        if name == super::package::binary_name("neomax")
            || AUXILIARIES
                .iter()
                .any(|candidate| name == super::package::binary_name(candidate))
            || (cfg!(windows)
                && ALIASES
                    .iter()
                    .any(|candidate| name == super::package::binary_name(candidate)))
        {
            return Some(ManifestKind::File);
        }
        if ALIASES
            .iter()
            .skip(1)
            .any(|candidate| name == super::package::binary_name(candidate))
        {
            return Some(ManifestKind::Symlink);
        }
        return None;
    }
    if path.strip_prefix("share/neomax/").is_some()
        && (ASSETS
            .iter()
            .any(|name| path == format!("share/neomax/{name}"))
            || DOCS
                .iter()
                .any(|name| path == format!("share/neomax/{name}"))
            || SHELL_ASSETS
                .iter()
                .any(|name| path == format!("share/neomax/{name}"))
            || WORKFLOWS
                .iter()
                .any(|name| path == format!("share/neomax/workflows/{name}"))
            || path == format!("share/neomax/{KIMI_AGENT_ASSET}"))
    {
        return Some(ManifestKind::File);
    }
    None
}

pub(crate) fn installed_path(paths: &InstallPaths, relative: &str) -> std::path::PathBuf {
    if let Some(name) = relative.strip_prefix("bin/") {
        paths.bin_dir.join(name)
    } else {
        paths
            .share_dir
            .join(relative.strip_prefix("share/neomax/").unwrap_or(relative))
    }
}
