use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const INSTALL_SCHEMA_VERSION: u32 = 1;
pub const PRODUCT: &str = "neomax";
pub const ALIASES: &[&str] = &[
    "neomax",
    "neomax-cli",
    "cmax",
    "cdx",
    "cdxmax",
    "ocx",
    "ocmax",
    "kmx",
    "kmax",
    "gmx",
    "gmax",
];
pub const AUXILIARIES: &[&str] = &["neomax-portal", "neomax-usage-agent", "neomax-worktrees"];
pub const ASSETS: &[&str] = &["opencode-model-policy.json", "LICENSE", "README.md"];
pub const SHELL_ASSETS: &[&str] = &[
    "shell/neomax-aliases.zsh",
    "shell/neomax-shell-shortcuts.sh",
];
pub const DOCS: &[&str] = &["INSTALLATION.md"];
pub const WORKFLOWS: &[&str] = &[
    "neomax.md",
    "rotate.md",
    "find-issues.md",
    "fix-issues.md",
    "project.md",
];
pub const KIMI_AGENT_ASSET: &str = "agents/neomax-kimi.md";
pub const KIMI_AGENT_RECORD: &str = "kimi-agent.md";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallOptions {
    pub package_root: Option<PathBuf>,
    pub paths: Option<super::paths::InstallPaths>,
    pub profile_home: Option<PathBuf>,
    pub force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallOptions {
    pub paths: Option<super::paths::InstallPaths>,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallReport {
    pub product: String,
    pub version: String,
    pub bin_dir: PathBuf,
    pub share_dir: PathBuf,
    pub aliases: Vec<String>,
    pub auxiliaries: Vec<String>,
    pub upgraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UninstallReport {
    pub product: String,
    pub bin_dir: PathBuf,
    pub share_dir: PathBuf,
    pub removed: Vec<String>,
    pub preserved: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstallManifest {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ManifestFile {
    pub path: String,
    pub kind: ManifestKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ManifestKind {
    File,
    Symlink,
}
