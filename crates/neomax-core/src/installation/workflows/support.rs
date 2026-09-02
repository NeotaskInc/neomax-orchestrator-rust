use std::path::{Path, PathBuf};

use crate::io::path_to_utf8;
use crate::runtime::RuntimeEnvironment;
use crate::{Error, Result};

pub(super) fn absolute_profile_path(path: PathBuf, home: &Path) -> Option<PathBuf> {
    if crate::io::is_rooted_but_not_absolute(&path) {
        return None;
    }
    Some(if path.is_absolute() {
        path
    } else {
        home.join(path)
    })
}

pub(super) fn profile_home() -> Result<PathBuf> {
    RuntimeEnvironment::process()
        .home_dir()
        .ok_or_else(|| Error::InvalidArgument("HOME or USERPROFILE is not set".into()))
}

pub(super) fn shell_quote(path: &Path) -> Result<String> {
    let value = path_to_utf8("workflow hook executable path", path)?;
    if cfg!(windows) {
        Ok(format!("\"{}\"", value.replace('"', "\\\"")))
    } else {
        Ok(format!("'{}'", value.replace('\'', "'\\''")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_normal_relative_and_absolute_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        assert_eq!(
            absolute_profile_path(PathBuf::from(".claude"), &home),
            Some(home.join(".claude"))
        );
        let absolute = temp.path().join("neomax-profile");
        assert_eq!(
            absolute_profile_path(absolute.clone(), &home),
            Some(absolute)
        );
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_profiles_that_depend_on_the_current_drive_directory() {
        let home = Path::new(r"C:\Users\profile");
        for path in [
            PathBuf::from(r"\rooted-profile"),
            PathBuf::from(r"C:drive-relative"),
        ] {
            assert_eq!(absolute_profile_path(path, home), None);
        }
    }
}
