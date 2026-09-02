use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

use super::files::{path_exists, read_bounded, sha256};
use super::paths::PackageRoot;
use super::types::{
    ALIASES, ASSETS, AUXILIARIES, DOCS, KIMI_AGENT_ASSET, PRODUCT, SHELL_ASSETS, WORKFLOWS,
};

const MAX_KIMI_AGENT_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    pub root: PathBuf,
    pub version: String,
}

impl Package {
    pub(crate) fn load(root: &PackageRoot) -> Result<Self> {
        let _root_guard = crate::io::PathGuard::for_directory(root.path())?;
        let root = root.path().to_path_buf();
        for name in ALIASES.iter().skip(1) {
            require_alias(&root.join("bin").join(binary_name(name)))?;
        }
        for name in AUXILIARIES {
            let path = root.join("bin").join(binary_name(name));
            require_regular_file(&path)?;
        }
        require_regular_file(&root.join("bin").join(binary_name("neomax")))?;
        for name in ASSETS {
            let path = if *name == "opencode-model-policy.json" {
                root.join("share").join(PRODUCT).join(name)
            } else {
                root.join(name)
            };
            require_regular_file(&path)?;
        }
        for name in SHELL_ASSETS {
            require_regular_file(&root.join("share").join(PRODUCT).join(name))?;
        }
        for name in DOCS {
            require_regular_file(&root.join("docs").join(name))?;
        }
        for name in WORKFLOWS {
            require_regular_file(
                &root
                    .join("share")
                    .join(PRODUCT)
                    .join("workflows")
                    .join(name),
            )?;
        }
        validate_kimi_agent(&root.join("share").join(PRODUCT).join(KIMI_AGENT_ASSET))?;
        let version = read_version(&root)?.unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
        Ok(Self { root, version })
    }

    pub(crate) fn binary(&self, name: &str) -> PathBuf {
        self.root.join("bin").join(binary_name(name))
    }

    pub(crate) fn asset(&self, name: &str) -> PathBuf {
        if name == "opencode-model-policy.json"
            || name.starts_with("agents/")
            || name.starts_with("shell/")
        {
            self.root.join("share").join(PRODUCT).join(name)
        } else {
            self.root.join(name)
        }
    }

    pub(crate) fn doc(&self, name: &str) -> PathBuf {
        self.root.join("docs").join(name)
    }

    pub(crate) fn workflow(&self, name: &str) -> PathBuf {
        self.root
            .join("share")
            .join(PRODUCT)
            .join("workflows")
            .join(name)
    }
}

pub(crate) fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn require_regular_file(path: &Path) -> Result<()> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    if !path_exists(path) {
        return Err(Error::NotFound(format!(
            "package file does not exist: {}",
            path.display()
        )));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(Error::InvalidArgument(format!(
            "package file is not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

pub(crate) fn validate_kimi_agent(path: &Path) -> Result<()> {
    require_regular_file(path)?;
    let bytes = read_bounded(path, MAX_KIMI_AGENT_BYTES)?;
    let content = std::str::from_utf8(&bytes).map_err(|error| Error::InvalidState {
        path: path.to_path_buf(),
        message: format!("Kimi agent asset is not valid UTF-8: {error}"),
    })?;
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return Err(Error::InvalidState {
            path: path.to_path_buf(),
            message: "Kimi agent asset must start with Markdown frontmatter".into(),
        });
    }
    let mut frontmatter = Vec::new();
    let mut closed = false;
    for line in &mut lines {
        if line == "---" {
            closed = true;
            break;
        }
        frontmatter.push(line);
    }
    if !closed
        || !frontmatter.contains(&"name: neomax")
        || !frontmatter.iter().any(|line| {
            line.strip_prefix("description:")
                .is_some_and(|description| !description.trim().is_empty())
        })
        || !lines.any(|line| line.contains("${base_prompt}"))
    {
        return Err(Error::InvalidState {
            path: path.to_path_buf(),
            message: "Kimi agent asset must contain valid frontmatter and ${base_prompt}".into(),
        });
    }
    if content.contains("kimi_cli.tools") || content.contains("multiagent:Task") {
        return Err(Error::InvalidState {
            path: path.to_path_buf(),
            message: "Kimi agent asset contains obsolete tool identifiers".into(),
        });
    }
    Ok(())
}

fn require_alias(path: &Path) -> Result<()> {
    if !path_exists(path) {
        return Err(Error::NotFound(format!(
            "package alias does not exist: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        if std::fs::read_link(path).ok().as_deref() == Some(Path::new("neomax")) {
            Ok(())
        } else {
            Err(Error::InvalidArgument(format!(
                "package alias is not a relative link to neomax: {}",
                path.display()
            )))
        }
    }
    #[cfg(windows)]
    {
        require_regular_file(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        require_regular_file(path)
    }
}

fn read_version(root: &Path) -> Result<Option<String>> {
    let path = root.join("RELEASE-MANIFEST.json");
    if !path.is_file() {
        return Ok(None);
    }
    if !fs::symlink_metadata(&path)?.file_type().is_file() {
        return Err(Error::InvalidState {
            path,
            message: "package release manifest must be a regular file".into(),
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&read_bounded(&path, 2 * 1024 * 1024)?)
        .map_err(|error| Error::InvalidState {
        path: path.clone(),
        message: error.to_string(),
    })?;
    if value.get("product").and_then(serde_json::Value::as_str) != Some(PRODUCT) {
        return Err(Error::InvalidState {
            path,
            message: "package manifest product does not match neomax".into(),
        });
    }
    validate_release_manifest(root, &value)?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Error::InvalidState {
            path: root.join("RELEASE-MANIFEST.json"),
            message: "package manifest is missing a version".into(),
        })?;
    if version.is_empty() || version.contains('/') || version.contains('\\') {
        return Err(Error::InvalidState {
            path: root.join("RELEASE-MANIFEST.json"),
            message: "package manifest version is invalid".into(),
        });
    }
    Ok(Some(version.to_owned()))
}

fn validate_release_manifest(root: &Path, value: &serde_json::Value) -> Result<()> {
    let files = value
        .get("files")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::InvalidState {
            path: root.join("RELEASE-MANIFEST.json"),
            message: "package manifest is missing files".into(),
        })?;
    let mut expected = ALIASES
        .iter()
        .chain(AUXILIARIES.iter())
        .map(|name| format!("bin/{}", binary_name(name)))
        .chain(ASSETS.iter().map(|name| {
            if *name == "opencode-model-policy.json" {
                format!("share/neomax/{name}")
            } else {
                (*name).into()
            }
        }))
        .chain(
            SHELL_ASSETS
                .iter()
                .map(|name| format!("share/neomax/{name}")),
        )
        .chain(std::iter::once(format!("share/neomax/{KIMI_AGENT_ASSET}")))
        .chain(DOCS.iter().map(|name| format!("docs/{name}")))
        .chain(
            WORKFLOWS
                .iter()
                .map(|name| format!("share/neomax/workflows/{name}")),
        )
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    for item in files {
        let path = item
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| Error::InvalidState {
                path: root.join("RELEASE-MANIFEST.json"),
                message: "package manifest contains a file without a path".into(),
            })?;
        if !actual.insert(path.to_owned()) {
            return Err(Error::InvalidState {
                path: root.join("RELEASE-MANIFEST.json"),
                message: format!("package manifest repeats {path}"),
            });
        }
        if !expected.contains(path) {
            return Err(Error::InvalidState {
                path: root.join("RELEASE-MANIFEST.json"),
                message: format!("package manifest contains an unexpected path {path}"),
            });
        }
        let source = root.join(path);
        let kind = item
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match kind {
            "file" => {
                let expected_hash = item
                    .get("sha256")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| Error::InvalidState {
                        path: root.join("RELEASE-MANIFEST.json"),
                        message: format!("package manifest is missing the hash for {path}"),
                    })?;
                let regular = std::fs::symlink_metadata(&source)
                    .map(|metadata| metadata.file_type().is_file())
                    .unwrap_or(false);
                if !regular || sha256(&source)? != expected_hash {
                    return Err(Error::InvalidState {
                        path: source,
                        message: "package file hash does not match its release manifest".into(),
                    });
                }
            }
            "symlink" => {
                if item.get("target").and_then(serde_json::Value::as_str) != Some("neomax")
                    || std::fs::read_link(&source).ok().as_deref() != Some(Path::new("neomax"))
                {
                    return Err(Error::InvalidState {
                        path: source,
                        message: "package alias is not a safe relative link".into(),
                    });
                }
            }
            other => {
                return Err(Error::InvalidState {
                    path: root.join("RELEASE-MANIFEST.json"),
                    message: format!("unsupported package manifest entry kind {other}"),
                });
            }
        }
    }
    if actual != expected {
        expected.retain(|path| !actual.contains(path));
        return Err(Error::InvalidState {
            path: root.join("RELEASE-MANIFEST.json"),
            message: format!("package manifest command surface differs: missing {expected:?}"),
        });
    }
    Ok(())
}
