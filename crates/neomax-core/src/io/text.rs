use std::ffi::OsStr;
use std::path::Path;

use crate::{Error, Result};

/// Convert an operating-system string only when it can be represented exactly.
pub fn os_str_to_utf8<'a>(label: &str, value: &'a OsStr) -> Result<&'a str> {
    value
        .to_str()
        .ok_or_else(|| Error::InvalidArgument(format!("{label} is not valid UTF-8")))
}

/// Convert a path only when its serialized form is lossless.
pub fn path_to_utf8<'a>(label: &str, path: &'a Path) -> Result<&'a str> {
    os_str_to_utf8(label, path.as_os_str())
}

pub fn path_to_string(label: &str, path: &Path) -> Result<String> {
    path_to_utf8(label, path).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_utf8_paths_without_rewriting_them() {
        let path = Path::new("/tmp/Neomax é");
        assert_eq!(path_to_string("path", path).unwrap(), "/tmp/Neomax é");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_os_strings() {
        use std::os::unix::ffi::OsStrExt;

        let value = OsStr::from_bytes(b"/tmp/neomax-\xff");
        let error = os_str_to_utf8("security-sensitive path", value).unwrap_err();
        assert!(error.to_string().contains("security-sensitive path"));
        assert!(error.to_string().contains("UTF-8"));
    }
}
