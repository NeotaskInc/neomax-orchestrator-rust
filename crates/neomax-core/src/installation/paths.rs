use std::env;
use std::path::{Path, PathBuf};

use crate::runtime::{RuntimeEnvironment, RuntimePlatform};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    pub root: PathBuf,
    pub bin_dir: PathBuf,
    pub share_dir: PathBuf,
    pub lock_path: PathBuf,
}

impl InstallPaths {
    pub fn discover() -> Result<Self> {
        Self::discover_from(&RuntimeEnvironment::process())
    }

    pub fn discover_from(environment: &RuntimeEnvironment) -> Result<Self> {
        let home = environment
            .home_dir()
            .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))?;
        let platform = environment.platform();
        require_absolute_root("HOME", &home, platform)?;
        let root = explicit_root(environment, "NEOMAX_INSTALL_ROOT")?.unwrap_or_else(|| {
            if platform.is_windows() {
                configured_root(environment.value("LOCALAPPDATA"), platform)
                    .unwrap_or_else(|| home.join("AppData").join("Local"))
                    .join("Neomax")
            } else {
                home.join(".local")
            }
        });
        let bin_dir = explicit_root(environment, "NEOMAX_INSTALL_BIN")?
            .unwrap_or_else(|| root.join("bin"));
        let share_dir = explicit_root(environment, "NEOMAX_INSTALL_SHARE")?
            .unwrap_or_else(|| root.join("share").join("neomax"));
        Self::new(root, bin_dir, share_dir)
    }

    pub fn neomax_binary(&self) -> PathBuf {
        self.neomax_binary_for(RuntimePlatform::current())
    }

    pub fn neomax_binary_for(&self, platform: RuntimePlatform) -> PathBuf {
        self.bin_dir.join(if platform.is_windows() {
            "neomax.exe"
        } else {
            "neomax"
        })
    }

    pub fn new(
        root: impl Into<PathBuf>,
        bin_dir: impl Into<PathBuf>,
        share_dir: impl Into<PathBuf>,
    ) -> Result<Self> {
        let root = root.into();
        let bin_dir = bin_dir.into();
        let share_dir = share_dir.into();
        if root.as_os_str().is_empty()
            || bin_dir.as_os_str().is_empty()
            || share_dir.as_os_str().is_empty()
        {
            return Err(Error::InvalidArgument(
                "installation paths may not be empty".into(),
            ));
        }
        for (label, path) in [
            ("installation root", &root),
            ("installation bin directory", &bin_dir),
            ("installation share directory", &share_dir),
        ] {
            if rooted_without_absolute(path, RuntimePlatform::current()) {
                return Err(Error::InvalidArgument(format!(
                    "{label} must not be rooted without an absolute prefix: {}",
                    path.display()
                )));
            }
        }
        let lock_parent = root.parent().unwrap_or(Path::new("."));
        Ok(Self {
            lock_path: lock_parent.join(".neomax-install.lock"),
            root,
            bin_dir,
            share_dir,
        })
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.share_dir.join("install-manifest.json")
    }

    pub fn asset_path(&self, name: &str) -> PathBuf {
        self.share_dir.join(name)
    }

    pub fn workflow_path(&self, name: &str) -> PathBuf {
        self.share_dir.join("workflows").join(name)
    }

    pub fn workflow_manifest_path(&self) -> PathBuf {
        self.share_dir.join("workflow-install-manifest.json")
    }

    pub(crate) fn validate_destinations(&self) -> Result<()> {
        for path in [&self.root, &self.bin_dir, &self.share_dir] {
            validate_destination_path(path)?;
        }
        Ok(())
    }
}

fn validate_destination_path(path: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return Ok(());
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(Error::Conflict(format!(
                "installation destination is not a real directory: {}",
                path.display()
            )));
        }
        Ok(())
    }

    #[cfg(windows)]
    {
    let _path_guard = crate::io::PathGuard::for_directory(path).map_err(|error| {
        Error::Conflict(format!(
            "installation destination cannot be opened safely: {}: {error}",
            path.display()
        ))
    })?;
    let mut current = Some(path);
    while let Some(candidate) = current {
        match std::fs::symlink_metadata(candidate) {
            Ok(metadata)
                if metadata.file_type().is_symlink() || is_reparse_point(&metadata) =>
            {
                return Err(Error::Conflict(format!(
                    "installation destination contains a reparse point: {}",
                    candidate.display()
                )));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(Error::Conflict(format!(
                    "installation destination is not a real directory: {}",
                    candidate.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        current = candidate.parent().filter(|parent| *parent != candidate);
    }
    Ok(())
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRoot(pub PathBuf);

impl PackageRoot {
    pub fn discover() -> Result<Self> {
        if let Some(path) = env::var_os("NEOMAX_PACKAGE_ROOT") {
            let path = PathBuf::from(path);
            require_absolute_root(
                "NEOMAX_PACKAGE_ROOT",
                &path,
                RuntimePlatform::current(),
            )?;
            return Self::new(path);
        }
        let executable = env::current_exe().map_err(|error| {
            Error::Message(format!("could not determine current executable: {error}"))
        })?;
        let parent = executable
            .parent()
            .ok_or_else(|| Error::InvalidArgument("current executable has no parent".into()))?;
        let root = if parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                if cfg!(windows) {
                    name.eq_ignore_ascii_case("bin")
                } else {
                    name == "bin"
                }
            })
        {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        };
        require_absolute_root("package root", root, RuntimePlatform::current())?;
        Self::new(root)
    }

    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(Error::InvalidArgument(
                "package root may not be empty".into(),
            ));
        }
        if rooted_without_absolute(&path, RuntimePlatform::current()) {
            return Err(Error::InvalidArgument(format!(
                "package root must not be rooted without an absolute prefix: {}",
                path.display()
            )));
        }
        let _path_guard = crate::io::PathGuard::for_directory(&path)?;
        if !path.is_dir() {
            return Err(Error::NotFound(format!(
                "package root does not exist: {}",
                path.display()
            )));
        }
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

fn explicit_root(
    environment: &RuntimeEnvironment,
    name: &str,
) -> Result<Option<PathBuf>> {
    environment
        .value(name)
        .map(|value| {
            let path = PathBuf::from(value);
            require_absolute_root(name, &path, environment.platform()).map(|()| path)
        })
        .transpose()
}

fn configured_root(value: Option<&str>, platform: RuntimePlatform) -> Option<PathBuf> {
    let path = value
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)?;
    (!rooted_without_absolute(&path, platform) && absolute_for_platform(&path, platform))
        .then_some(path)
}

fn require_absolute_root(
    label: &str,
    path: &Path,
    platform: RuntimePlatform,
) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(Error::InvalidArgument(format!(
            "{label} must be an absolute path"
        )));
    }
    if rooted_without_absolute(path, platform) {
        return Err(Error::InvalidArgument(format!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        )));
    }
    if !absolute_for_platform(path, platform) {
        return Err(Error::InvalidArgument(format!(
            "{label} must be an absolute path: {}",
            path.display()
        )));
    }
    Ok(())
}

fn rooted_without_absolute(path: &Path, platform: RuntimePlatform) -> bool {
    crate::io::is_rooted_but_not_absolute(path)
        || (platform.is_windows() && {
            let raw = path.to_string_lossy();
            let bytes = raw.as_bytes();
            (raw.starts_with(['\\', '/'])
                && !raw.starts_with("\\\\")
                && !raw.starts_with("//"))
                || (bytes.get(1) == Some(&b':')
                    && !bytes
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))
        })
}

fn absolute_for_platform(path: &Path, platform: RuntimePlatform) -> bool {
    path.is_absolute()
        || (platform.is_windows() && {
            let raw = path.to_string_lossy();
            let bytes = raw.as_bytes();
            raw.starts_with("\\\\")
                || raw.starts_with("//")
                || (bytes.get(1) == Some(&b':')
                    && bytes
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeEnvironment;

    #[test]
    fn windows_install_paths_follow_localappdata_and_preserve_explicit_overrides() {
        let environment = RuntimeEnvironment::fixture(
            RuntimePlatform::Windows,
            [
                ("USERPROFILE".into(), "C:\\Users\\J\u{00f6}rg Space".into()),
                ("LOCALAPPDATA".into(), "D:\\Local Space".into()),
                ("NEOMAX_INSTALL_ROOT".into(), "E:\\Neomax Root".into()),
            ],
            "C:\\work",
        );
        let paths = InstallPaths::discover_from(&environment).unwrap();
        let root = PathBuf::from("E:\\Neomax Root");
        assert_eq!(paths.root, root);
        assert_eq!(paths.bin_dir, root.join("bin"));
        assert_eq!(
            paths.neomax_binary_for(RuntimePlatform::Windows),
            root.join("bin").join("neomax.exe")
        );
    }

    #[test]
    fn explicit_installation_roots_require_absolute_paths() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let environment = RuntimeEnvironment::fixture(
            RuntimePlatform::Unix,
            [
                ("HOME".into(), home.to_string_lossy().into_owned()),
                ("NEOMAX_INSTALL_ROOT".into(), "relative-install".into()),
            ],
            temp.path(),
        );
        let error = InstallPaths::discover_from(&environment).unwrap_err();
        assert!(error.to_string().contains("NEOMAX_INSTALL_ROOT"));
        assert!(error.to_string().contains("absolute"));

        for key in ["NEOMAX_INSTALL_BIN", "NEOMAX_INSTALL_SHARE"] {
            let environment = RuntimeEnvironment::fixture(
                RuntimePlatform::Unix,
                [
                    ("HOME".into(), home.to_string_lossy().into_owned()),
                    (key.into(), "relative-install".into()),
                ],
                temp.path(),
            );
            let error = InstallPaths::discover_from(&environment).unwrap_err();
            assert!(error.to_string().contains(key));
            assert!(error.to_string().contains("absolute"));
        }
    }

    #[test]
    fn invalid_localappdata_falls_back_to_the_home_root() {
        let environment = RuntimeEnvironment::fixture(
            RuntimePlatform::Windows,
            [
                ("USERPROFILE".into(), r"C:\Users\fixture".into()),
                ("LOCALAPPDATA".into(), r"C:drive-relative".into()),
            ],
            r"C:\work",
        );
        let paths = InstallPaths::discover_from(&environment).unwrap();
        let home = PathBuf::from(r"C:\Users\fixture");
        assert_eq!(
            paths.root,
            home.join("AppData").join("Local").join("Neomax")
        );
    }

    #[test]
    fn windows_absolute_installation_overrides_are_portable_injected_values() {
        let environment = RuntimeEnvironment::fixture(
            RuntimePlatform::Windows,
            [
                ("USERPROFILE".into(), r"C:\Users\fixture".into()),
                ("NEOMAX_INSTALL_ROOT".into(), r"D:\Neomax".into()),
                ("NEOMAX_INSTALL_BIN".into(), r"E:\Neomax\bin".into()),
                ("NEOMAX_INSTALL_SHARE".into(), r"F:\Neomax\share".into()),
            ],
            r"C:\work",
        );
        let paths = InstallPaths::discover_from(&environment).unwrap();
        assert_eq!(paths.root, PathBuf::from(r"D:\Neomax"));
        assert_eq!(paths.bin_dir, PathBuf::from(r"E:\Neomax\bin"));
        assert_eq!(paths.share_dir, PathBuf::from(r"F:\Neomax\share"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_partial_installation_overrides_fail_closed() {
        for key in [
            "NEOMAX_INSTALL_ROOT",
            "NEOMAX_INSTALL_BIN",
            "NEOMAX_INSTALL_SHARE",
        ] {
            let environment = RuntimeEnvironment::fixture(
                RuntimePlatform::Windows,
                [
                    ("USERPROFILE".into(), r"C:\Users\fixture".into()),
                    (key.into(), r"\rooted".into()),
                ],
                r"C:\work",
            );
            assert!(InstallPaths::discover_from(&environment).is_err());
        }
        assert!(InstallPaths::new(
            PathBuf::from(r"\rooted"),
            PathBuf::from(r"C:\Neomax\bin"),
            PathBuf::from(r"C:\Neomax\share"),
        )
        .is_err());
        assert!(InstallPaths::new(
            PathBuf::from(r"C:drive-relative"),
            PathBuf::from(r"C:\Neomax\bin"),
            PathBuf::from(r"C:\Neomax\share"),
        )
        .is_err());
    }

    #[test]
    fn package_root_new_preserves_normal_relative_paths() {
        assert!(PackageRoot::new(".").is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn package_root_rejects_windows_partial_paths() {
        for value in [r"\rooted", r"C:drive-relative"] {
            assert!(PackageRoot::new(value).is_err());
        }
    }
}
