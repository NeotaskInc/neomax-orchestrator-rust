use std::io;
use std::path::{Component, Path};

#[cfg(windows)]
use std::fs::{self, File, OpenOptions};
#[cfg(windows)]
use std::path::PathBuf;

/// Holds non-delete-sharing handles for the directory components of a path.
///
/// On Windows, keeping these handles open prevents a checked directory from
/// being replaced by a junction or another reparse point before the caller's
/// final operation runs.
#[derive(Debug, Default)]
pub struct PathGuard {
    #[cfg(windows)]
    _handles: Vec<File>,
}

pub fn is_rooted_but_not_absolute(path: &Path) -> bool {
    !path.is_absolute()
        && (path.has_root()
            || path
                .components()
                .any(|component| matches!(component, Component::Prefix(_))))
}

impl PathGuard {
    pub fn for_path(path: &Path) -> io::Result<Self> {
        Self::open(path, false)
    }

    pub fn for_directory(path: &Path) -> io::Result<Self> {
        Self::open(path, true)
    }

    pub fn for_existing_parent(path: &Path) -> io::Result<Self> {
        Self::open_existing_parent(path)
    }

    /// Ensure a directory exists and keep a non-delete-sharing handle for
    /// every component until the returned guard is dropped.
    ///
    /// `create_dir_all` is not sufficient for operations that subsequently
    /// rename a file into the directory on Windows: a checked component can
    /// otherwise be replaced by a junction between the check and the rename.
    /// Components are created from the root toward the leaf and each newly
    /// created directory is opened and validated before the next component is
    /// touched.
    pub fn ensure_directory(path: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            return Self::ensure_directory_windows(path);
        }

        #[cfg(not(windows))]
        {
            std::fs::create_dir_all(path)?;
            Ok(Self {})
        }
    }

    #[cfg(windows)]
    fn open(path: &Path, include_final: bool) -> io::Result<Self> {
        let mut paths = existing_components(path, include_final)?;
        paths.dedup();
        let mut handles = Vec::with_capacity(paths.len());
        for component in paths {
            handles.push(open_directory_component(&component)?);
        }
        Ok(Self { _handles: handles })
    }

    #[cfg(windows)]
    fn open_existing_parent(path: &Path) -> io::Result<Self> {
        let paths = directory_components(&parent_or_dot(path));
        let mut handles = Vec::with_capacity(paths.len());
        for component in paths {
            handles.push(open_directory_component(&component)?);
        }
        Ok(Self { _handles: handles })
    }

    #[cfg(windows)]
    fn ensure_directory_windows(path: &Path) -> io::Result<Self> {
        let mut handles = Vec::new();
        for component in directory_components(path) {
            match fs::symlink_metadata(&component) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    match fs::create_dir(&component) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            }
            handles.push(open_directory_component(&component)?);
        }
        Ok(Self { _handles: handles })
    }

    #[cfg(not(windows))]
    fn open(_path: &Path, _include_final: bool) -> io::Result<Self> {
        Ok(Self {})
    }

    #[cfg(not(windows))]
    fn open_existing_parent(_path: &Path) -> io::Result<Self> {
        Ok(Self {})
    }
}

#[cfg(windows)]
fn open_directory_component(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = OpenOptions::new();
    options
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    let handle = options.open(path)?;
    let metadata = handle.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("path component is not a real directory: {}", path.display()),
        ));
    }
    Ok(handle)
}

#[cfg(windows)]
fn directory_components(path: &Path) -> Vec<PathBuf> {
    let mut components = path
        .ancestors()
        .filter(|component| !component.as_os_str().is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    components.reverse();
    components.dedup();
    components
}

#[cfg(windows)]
fn existing_components(path: &Path, include_final: bool) -> io::Result<Vec<PathBuf>> {
    let mut current = if include_final {
        path.to_path_buf()
    } else {
        parent_or_dot(path)
    };
    let mut components = Vec::new();
    loop {
        match fs::symlink_metadata(&current) {
            Ok(_) => components.push(current.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let Some(parent) = parent_of(&current) else {
            break;
        };
        if parent == current {
            break;
        }
        current = parent;
    }
    components.reverse();
    Ok(components)
}

#[cfg(windows)]
fn parent_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|parent| {
        if parent.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            parent.to_path_buf()
        }
    })
}

#[cfg(windows)]
fn parent_or_dot(path: &Path) -> PathBuf {
    parent_of(path).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::fs;

    #[test]
    fn ensure_directory_creates_missing_components() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("one").join("two");

        let _guard = PathGuard::ensure_directory(&directory).unwrap();

        assert!(directory.is_dir());
    }

    #[test]
    fn ordinary_relative_and_absolute_paths_are_not_partial_roots() {
        assert!(!is_rooted_but_not_absolute(Path::new("relative/path")));
        assert!(!is_rooted_but_not_absolute(
            tempfile::tempdir().unwrap().path()
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_relative_and_drive_relative_paths_are_partial_roots() {
        assert!(is_rooted_but_not_absolute(Path::new(r"\rooted")));
        assert!(is_rooted_but_not_absolute(Path::new(r"C:drive-relative")));
    }

    #[cfg(windows)]

    #[test]
    fn rejects_reparse_directory_components() {
        use std::os::windows::fs::symlink_dir;

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let link = temp.path().join("link");
        fs::create_dir(&target).unwrap();
        if symlink_dir(&target, &link).is_err() {
            return;
        }

        assert!(PathGuard::for_directory(&link).is_err());
        assert!(PathGuard::for_path(&link.join("file")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn accepts_regular_directory_components() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("directory");
        fs::create_dir(&directory).unwrap();

        assert!(PathGuard::for_directory(&directory).is_ok());
        assert!(PathGuard::for_path(&directory.join("file")).is_ok());
    }
}
