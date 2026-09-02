use super::*;
use neomax_core::Engine;

#[test]
fn rotation_options_keep_provider_scope_and_active_selection_separate() {
    let options = RotationOptions::parse(&[
        "--workers".into(),
        "claude+codex".into(),
        "--active".into(),
        "--run=run-1".into(),
    ])
    .unwrap();
    assert!(options.active);
    assert_eq!(options.ids, ["run-1".to_owned()]);
    assert_eq!(options.scope.unwrap().csv(), "claude,codex");

    let options = RotationOptions::parse(&["--engine=opencode".into()]).unwrap();
    assert!(options.scope.unwrap().contains(Engine::Opencode));
}

#[test]
fn rotation_options_reject_unknown_mutations_before_any_runtime_access() {
    assert!(RotationOptions::parse(&["--provider-call".into()]).is_err());
}
