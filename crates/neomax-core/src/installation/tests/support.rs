use std::fs;
use std::path::{Path, PathBuf};

use super::super::paths::InstallPaths;
use super::super::types::{
    ALIASES, ASSETS, AUXILIARIES, DOCS, KIMI_AGENT_ASSET, SHELL_ASSETS, WORKFLOWS,
};

pub(super) fn binary_path(root: impl AsRef<Path>, name: &str) -> PathBuf {
    root.as_ref().join(super::super::package::binary_name(name))
}

pub(super) fn fixture() -> (tempfile::TempDir, tempfile::TempDir, InstallPaths) {
    let package = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    fs::create_dir_all(package.path().join("bin")).unwrap();
    fs::create_dir_all(package.path().join("share/neomax")).unwrap();
    fs::create_dir_all(package.path().join("share/neomax/shell")).unwrap();
    fs::create_dir_all(package.path().join("share/neomax/workflows")).unwrap();
    fs::create_dir_all(package.path().join("share/neomax/agents")).unwrap();
    fs::create_dir_all(package.path().join("docs")).unwrap();
    for name in ALIASES.iter().skip(1) {
        let path = binary_path(package.path().join("bin"), name);
        #[cfg(unix)]
        std::os::unix::fs::symlink("neomax", path).unwrap();
        #[cfg(windows)]
        fs::write(path, format!("alias:{name}")).unwrap();
    }
    fs::write(
        binary_path(package.path().join("bin"), "neomax"),
        b"main-v1",
    )
    .unwrap();
    for name in AUXILIARIES {
        fs::write(
            binary_path(package.path().join("bin"), name),
            format!("aux:{name}"),
        )
        .unwrap();
    }
    for name in ASSETS {
        let path = if *name == "opencode-model-policy.json" {
            package.path().join("share/neomax").join(name)
        } else {
            package.path().join(name)
        };
        fs::write(path, format!("asset:{name}")).unwrap();
    }
    for name in SHELL_ASSETS {
        fs::write(
            package.path().join("share/neomax").join(name),
            format!("shell-asset:{name}"),
        )
        .unwrap();
    }
    for name in DOCS {
        fs::write(
            package.path().join("docs").join(name),
            format!("doc:{name}"),
        )
        .unwrap();
    }
    for name in WORKFLOWS {
        fs::write(
            package.path().join("share/neomax/workflows").join(name),
            format!("workflow:{name}"),
        )
        .unwrap();
    }
    fs::write(
        package.path().join("share/neomax").join(KIMI_AGENT_ASSET),
        "---\nname: neomax\ndescription: Neomax orchestration agent\n---\n${base_prompt}\n\nFollow the Neomax tool contract.\n",
    )
    .unwrap();
    let paths = InstallPaths::new(
        destination.path(),
        destination.path().join("bin"),
        destination.path().join("share/neomax"),
    )
    .unwrap();
    (package, destination, paths)
}
