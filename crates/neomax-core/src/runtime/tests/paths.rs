use std::path::PathBuf;

use crate::runtime::{RuntimeEnvironment, RuntimePlatform};

#[test]
fn opencode_windows_roots_use_local_data_and_roaming_config() {
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("USERPROFILE".into(), "C:\\Users\\J\u{00f6}rg".into()),
            ("APPDATA".into(), "D:\\Roaming Space".into()),
            ("LOCALAPPDATA".into(), "D:\\Local Space".into()),
        ],
        "C:\\work",
    );
    let profile = PathBuf::from("C:\\Users\\J\u{00f6}rg\\.opencode");
    assert_eq!(
        environment.opencode_data_dir(&profile),
        PathBuf::from("D:\\Local Space").join("opencode")
    );
    assert_eq!(
        environment.opencode_config_dir(),
        PathBuf::from("D:\\Roaming Space").join("opencode")
    );
}

#[test]
fn opencode_rejects_relative_xdg_roots_and_uses_the_home_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let work = temp.path().join("work");
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Unix,
        [
            ("HOME".into(), home.to_string_lossy().into_owned()),
            ("XDG_DATA_HOME".into(), "relative-data".into()),
            ("XDG_CONFIG_HOME".into(), "relative-config".into()),
        ],
        work,
    );
    let profile = home.join(".opencode");
    assert_eq!(
        environment.opencode_data_dir(&profile),
        home.join(".local/share/opencode")
    );
    assert_eq!(
        environment.opencode_config_dir(),
        home.join(".config/opencode")
    );
}

#[cfg(windows)]
#[test]
fn opencode_rejects_partial_windows_roots_and_uses_the_home_fallback() {
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("USERPROFILE".into(), r"C:\Users\fixture".into()),
            ("LOCALAPPDATA".into(), r"C:drive-relative".into()),
            ("APPDATA".into(), r"\rooted".into()),
        ],
        r"C:\fixture\work",
    );
    let profile = PathBuf::from(r"C:\Users\fixture\.opencode");
    assert_eq!(
        environment.opencode_data_dir(&profile),
        PathBuf::from(r"C:\Users\fixture\AppData\Local\opencode")
    );
    assert_eq!(
        environment.opencode_config_dir(),
        PathBuf::from(r"C:\Users\fixture\AppData\Roaming\opencode")
    );
}
