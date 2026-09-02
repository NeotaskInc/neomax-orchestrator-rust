use super::super::inspector::WindowsProcessInfo;
use super::*;

fn process(image_path: &str, command_line: Option<&str>) -> WindowsProcessInfo {
    WindowsProcessInfo {
        pid: 10,
        parent_pid: Some(1),
        image_path: image_path.into(),
        command_line: command_line.map(str::to_owned),
        config_dir: None,
    }
}

#[test]
fn only_exact_claude_images_and_known_shims_match() {
    assert!(is_claude_process(&process(r"C:\bin\claude.exe", None)));
    assert!(is_claude_process(&process(r"C:\bin\claude.cmd", None)));
    assert!(is_claude_process(&process(
        r"C:\node\node.exe",
        Some(r#"node "C:\node_modules\@anthropic-ai\claude-code\cli.js""#),
    )));
    assert!(is_claude_process(&process(
        r"C:\Windows\System32\cmd.exe",
        Some(r#"cmd.exe /d /s /c "C:\bin\claude.cmd""#),
    )));
    assert!(!is_claude_process(&process(
        r"C:\bin\claude-helper.exe",
        None,
    )));
    assert!(!is_claude_process(&process(
        r"C:\node\node.exe",
        Some("node --prompt claude"),
    )));
}

#[test]
fn command_line_profile_values_preserve_quoted_spaces() {
    assert_eq!(
        profile_environment_value(
            r#"claude.exe CLAUDE_CONFIG_DIR="C:\profiles\primary account""#,
            "CLAUDE_CONFIG_DIR",
        ),
        Some(r"C:\profiles\primary account".into())
    );
    assert_eq!(
        profile_environment_value(
            "claude.exe CLAUDE_CONFIG_DIR=/profiles/primary account OTHER=value",
            "CLAUDE_CONFIG_DIR",
        ),
        Some("/profiles/primary account".into())
    );
    assert_eq!(
        profile_environment_value("claude.exe OTHER=value", "CLAUDE_CONFIG_DIR"),
        None
    );
}

#[test]
fn windows_argument_parser_keeps_quoted_paths_together() {
    assert_eq!(
        windows_arguments(r#"node "C:\Program Files\claude\cli.js" --prompt "hello world""#),
        vec![
            "node".to_owned(),
            r"C:\Program Files\claude\cli.js".to_owned(),
            "--prompt".to_owned(),
            "hello world".to_owned(),
        ]
    );
}

#[test]
fn environment_end_accepts_a_normal_utf16le_ascii_block() {
    let bytes = utf16_bytes("CLAUDE_CONFIG_DIR=/profiles/primary\0\0");
    assert_eq!(environment_end(&bytes), Some(bytes.len()));
}

#[test]
fn environment_end_rejects_truncated_and_unaligned_terminators() {
    let truncated = utf16_bytes("CLAUDE_CONFIG_DIR=/profiles/primary\0");
    assert_eq!(environment_end(&truncated), None);

    let unaligned = [b'x', 0, 0, 0, 0];
    assert_eq!(environment_end(&unaligned), None);
}

fn utf16_bytes(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}
