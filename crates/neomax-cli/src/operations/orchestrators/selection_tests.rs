use std::path::PathBuf;

use super::*;

#[test]
fn parse_pick_neomax_supports_priority_record_and_resume() {
    let parsed = parse_pick_options(
        &[
            "--priority".into(),
            "opencode+kimi".into(),
            "--cwd=/workspace".into(),
            "--resume".into(),
            "--record".into(),
            "--json".into(),
        ],
        true,
    )
    .unwrap();
    assert_eq!(parsed.priority.as_deref(), Some("opencode+kimi"));
    assert_eq!(parsed.cwd, Some(PathBuf::from("/workspace")));
    assert!(parsed.resume);
    assert!(parsed.record);
    assert!(parsed.json);
}

#[test]
fn pick_orchestrator_options_reject_selection_state_flags() {
    assert!(parse_pick_options(&["--record".into()], false).is_err());
    assert!(parse_pick_options(&["--priority".into(), "grok".into()], false).is_err());
}
