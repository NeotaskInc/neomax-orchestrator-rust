use crate::Engine;
use crate::io::ProcessOutput;

use super::super::{ProbeState, classify_process_output, command_matches_engine};

#[test]
fn provider_process_matching_accepts_wrappers_and_native_children() {
    assert!(command_matches_engine(
        "node /opt/codex exec",
        Engine::Codex
    ));
    assert!(command_matches_engine("opencode run", Engine::Claude));
    assert!(!command_matches_engine("sleep 30", Engine::Claude));
}

#[test]
fn failed_process_listing_is_not_proof_of_a_dead_or_live_worker() {
    assert_eq!(
        classify_process_output(None, Engine::Codex),
        ProbeState::Unknown
    );
    let failed = ProcessOutput {
        status_code: Some(1),
        success: false,
        stdout: Vec::new(),
        stderr: b"permission denied".to_vec(),
        timed_out: false,
        stdout_truncated: false,
        stderr_truncated: false,
    };
    assert_eq!(
        classify_process_output(Some(&failed), Engine::Codex),
        ProbeState::Unknown
    );
    let unrelated = ProcessOutput {
        status_code: Some(0),
        success: true,
        stdout: b"sleep 30\n".to_vec(),
        stderr: Vec::new(),
        timed_out: false,
        stdout_truncated: false,
        stderr_truncated: false,
    };
    assert_eq!(
        classify_process_output(Some(&unrelated), Engine::Codex),
        ProbeState::Dead
    );
}

#[cfg(unix)]
#[test]
fn failed_process_group_listing_is_unknown() {
    assert_eq!(
        super::super::classify_group_output(None, 123, Engine::Codex),
        ProbeState::Unknown
    );
}

#[cfg(unix)]
#[test]
fn invalid_zero_pid_is_unknown_instead_of_a_process_group_probe() {
    assert_eq!(super::super::pid_state(0), ProbeState::Unknown);
}
