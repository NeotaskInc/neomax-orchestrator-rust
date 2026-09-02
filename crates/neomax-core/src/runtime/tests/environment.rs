use std::path::PathBuf;

use crate::runtime::{RuntimeEnvironment, RuntimePlatform};

#[test]
fn windows_home_prefers_userprofile_and_preserves_unicode() {
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("HOME".into(), "C:\\compat\\home".into()),
            ("USERPROFILE".into(), "C:\\Users\\J\u{00f6}rg Space".into()),
        ],
        "C:\\work",
    );
    assert_eq!(
        environment.home_dir(),
        Some(PathBuf::from("C:\\Users\\J\u{00f6}rg Space"))
    );
    assert_eq!(
        environment.resolve_path("~\\project\\source"),
        PathBuf::from("C:\\Users\\J\u{00f6}rg Space")
            .join("project")
            .join("source")
    );
}

#[test]
fn safe_windows_environment_keeps_runtime_roots_and_provider_config_only() {
    let environment = RuntimeEnvironment::fixture(
        RuntimePlatform::Windows,
        [
            ("USERPROFILE".into(), "C:\\Users\\J\u{00f6}rg Space".into()),
            (
                "APPDATA".into(),
                "C:\\Users\\J\u{00f6}rg Space\\Roaming".into(),
            ),
            (
                "LOCALAPPDATA".into(),
                "C:\\Users\\J\u{00f6}rg Space\\Local".into(),
            ),
            ("SystemRoot".into(), "C:\\Windows".into()),
            ("ComSpec".into(), "C:\\Windows\\System32\\cmd.exe".into()),
            ("PATH".into(), "C:\\Tools;C:\\Windows\\System32".into()),
            ("TEMP".into(), "C:\\Temp Space".into()),
            ("OPENAI_API_KEY".into(), "secret".into()),
            ("CODEX_HOME".into(), "C:\\Profiles\\one".into()),
        ],
        "C:\\work",
    );
    let safe = environment.safe_child_environment(Some("CODEX_HOME"));
    assert_eq!(
        safe.get("USERPROFILE"),
        Some(&"C:\\Users\\J\u{00f6}rg Space".into())
    );
    assert_eq!(safe.get("TEMP"), Some(&"C:\\Temp Space".into()));
    assert_eq!(safe.get("CODEX_HOME"), Some(&"C:\\Profiles\\one".into()));
    assert!(!safe.contains_key("OPENAI_API_KEY"));
}
