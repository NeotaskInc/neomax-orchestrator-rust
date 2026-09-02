//! Cross-platform home, temporary, and provider data paths.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::platform::{RuntimePlatform, UNIX_CHILD_ENVIRONMENT, WINDOWS_CHILD_ENVIRONMENT};

pub fn native_home<F>(platform: RuntimePlatform, mut value: F) -> Option<PathBuf>
where
    F: FnMut(&str) -> Option<String>,
{
    let keys: &[&str] = if platform.is_windows() {
        &["USERPROFILE", "HOME"]
    } else {
        &["HOME", "USERPROFILE"]
    };
    keys.iter().find_map(|key| {
        value(key)
            .filter(|item| !item.trim().is_empty())
            .map(PathBuf::from)
            .filter(|path| !crate::io::is_rooted_but_not_absolute(path))
    })
}

pub fn temp_dir<F>(platform: RuntimePlatform, mut value: F) -> Option<PathBuf>
where
    F: FnMut(&str) -> Option<String>,
{
    let keys: &[&str] = if platform.is_windows() {
        &["TEMP", "TMP"]
    } else {
        &["TMPDIR"]
    };
    keys.iter().find_map(|key| {
        value(key)
            .filter(|item| !item.trim().is_empty())
            .map(PathBuf::from)
            .filter(|path| !crate::io::is_rooted_but_not_absolute(path))
    })
}

/// Expand only a leading `~` and resolve relative values against the injected
/// working directory. A tilde in the middle of a path remains literal.
pub fn resolve_path(
    value: &str,
    home: Option<&Path>,
    current_dir: &Path,
    platform: RuntimePlatform,
) -> PathBuf {
    let raw = value;
    let path = Path::new(raw);
    if crate::io::is_rooted_but_not_absolute(path) {
        return PathBuf::new();
    }
    let tilde = raw == "~"
        || (raw.starts_with('~')
            && raw
                .as_bytes()
                .get(1)
                .is_some_and(|separator| *separator == b'/' || *separator == b'\\'));
    if tilde {
        if let Some(home) = home {
            if crate::io::is_rooted_but_not_absolute(home) {
                return PathBuf::new();
            }
            let remainder = raw
                .strip_prefix('~')
                .unwrap_or_default()
                .trim_start_matches(['/', '\\']);
            return if remainder.is_empty() {
                home.to_path_buf()
            } else if platform.is_windows() {
                join_windows_path(home, remainder)
            } else {
                home.join(remainder)
            };
        }
    }
    if is_absolute(path, raw, platform) {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn is_absolute(path: &Path, raw: &str, platform: RuntimePlatform) -> bool {
    path.is_absolute()
        || (platform.is_windows()
            && (raw.starts_with("\\\\")
                || raw.starts_with('/')
                || (raw.as_bytes().get(1) == Some(&b':')
                    && raw
                        .as_bytes()
                        .get(2)
                        .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))))
}

fn join_windows_path(home: &Path, remainder: &str) -> PathBuf {
    let mut result = home.to_path_buf();
    for component in remainder.split(['/', '\\']).filter(|part| !part.is_empty()) {
        result.push(component);
    }
    result
}

pub fn safe_child_environment<F>(
    platform: RuntimePlatform,
    mut value: F,
    home: Option<&Path>,
    provider_config: Option<&str>,
) -> BTreeMap<String, String>
where
    F: FnMut(&str) -> Option<String>,
{
    let names: &[&str] = if platform.is_windows() {
        WINDOWS_CHILD_ENVIRONMENT
    } else {
        UNIX_CHILD_ENVIRONMENT
    };
    let mut output = BTreeMap::new();
    for name in names {
        if let Some(item) = value(name).filter(|item| !item.is_empty()) {
            output.insert((*name).into(), item);
        }
    }
    if let Some(home) = home {
        output.insert("HOME".into(), home.to_string_lossy().into_owned());
        if platform.is_windows() {
            output
                .entry("USERPROFILE".into())
                .or_insert_with(|| home.to_string_lossy().into_owned());
        }
    }
    if let Some(name) = provider_config.filter(|name| !name.is_empty()) {
        if let Some(item) = value(name).filter(|item| !item.is_empty()) {
            output.insert(name.into(), item);
        }
    }
    output
}

pub fn opencode_data_root<F>(home: &Path, platform: RuntimePlatform, mut value: F) -> PathBuf
where
    F: FnMut(&str) -> Option<String>,
{
    let root = if platform.is_windows() {
        configured_root(value("LOCALAPPDATA"), platform)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
    } else {
        configured_root(value("XDG_DATA_HOME"), platform)
            .unwrap_or_else(|| home.join(".local").join("share"))
    };
    root.join("opencode")
}

pub fn opencode_config_dir<F>(home: &Path, platform: RuntimePlatform, mut value: F) -> PathBuf
where
    F: FnMut(&str) -> Option<String>,
{
    let root = if platform.is_windows() {
        configured_root(value("APPDATA"), platform)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        configured_root(value("XDG_CONFIG_HOME"), platform)
            .unwrap_or_else(|| home.join(".config"))
    };
    root.join("opencode")
}

fn configured_root(value: Option<String>, platform: RuntimePlatform) -> Option<PathBuf> {
    let value = value?;
    if value.trim().is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    let raw = path.to_string_lossy();
    (!crate::io::is_rooted_but_not_absolute(&path) && is_absolute(&path, &raw, platform))
        .then_some(path)
}

pub fn opencode_data_dir<F>(
    profile: &Path,
    home: &Path,
    platform: RuntimePlatform,
    value: F,
) -> PathBuf
where
    F: FnMut(&str) -> Option<String>,
{
    let default_profile = home.join(".opencode");
    if same_path(profile, &default_profile, platform) {
        opencode_data_root(home, platform, value)
    } else {
        profile.join("opencode")
    }
}

fn same_path(left: &Path, right: &Path, platform: RuntimePlatform) -> bool {
    left == right
        || (platform.is_windows()
            && normalize_windows_text(left).eq_ignore_ascii_case(&normalize_windows_text(right)))
        || (left.is_absolute()
            && right.is_absolute()
            && left
                .to_string_lossy()
                .eq_ignore_ascii_case(&right.to_string_lossy()))
}

fn normalize_windows_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_keep_their_existing_resolution() {
        let current_dir = Path::new("fixture/workspace");
        assert_eq!(
            resolve_path(
                "project/source",
                Some(Path::new("fixture/home")),
                current_dir,
                RuntimePlatform::Unix,
            ),
            current_dir.join("project/source")
        );
    }

    #[cfg(unix)]
    #[test]
    fn absolute_paths_keep_their_existing_resolution() {
        let current_dir = Path::new("/fixture/workspace");
        assert_eq!(
            resolve_path(
                "/opt/neomax",
                Some(Path::new("/fixture/home")),
                current_dir,
                RuntimePlatform::Unix,
            ),
            PathBuf::from("/opt/neomax")
        );
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_roots_fail_closed_without_rehoming() {
        let current_dir = Path::new(r"C:\fixture\workspace");
        let home = Path::new(r"C:\fixture\home");
        for value in [r"\rooted", r"C:drive-relative"] {
            assert_eq!(
                resolve_path(value, Some(home), current_dir, RuntimePlatform::Windows),
                PathBuf::new()
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_homes_are_skipped_or_rejected() {
        assert_eq!(
            native_home(RuntimePlatform::Windows, |key| match key {
                "USERPROFILE" => Some(r"C:drive-relative".into()),
                "HOME" => Some(r"C:\fixture\home".into()),
                _ => None,
            }),
            Some(PathBuf::from(r"C:\fixture\home"))
        );
        assert_eq!(
            resolve_path(
                r"~\project",
                Some(Path::new(r"C:drive-relative")),
                Path::new(r"C:\fixture\workspace"),
                RuntimePlatform::Windows,
            ),
            PathBuf::new()
        );
    }
}
