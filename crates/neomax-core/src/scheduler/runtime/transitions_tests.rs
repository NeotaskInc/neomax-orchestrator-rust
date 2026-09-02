use crate::Engine;
use crate::scheduler::PartState;

use super::test_support::{part, pending_state, plan};
use super::transitions::{PartTransition, apply_transition};

#[test]
fn applies_the_normal_part_lifecycle() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let mut state = pending_state(&plan);
    assert_eq!(
        apply_transition(&mut state, "one", PartTransition::Start)
            .unwrap()
            .current,
        PartState::Running
    );
    assert_eq!(
        apply_transition(&mut state, "one", PartTransition::Complete)
            .unwrap()
            .current,
        PartState::Done
    );
}

#[test]
fn retries_release_a_failed_part_to_pending() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let mut state = pending_state(&plan);
    apply_transition(
        &mut state,
        "one",
        PartTransition::Fail {
            error: "quota".into(),
        },
    )
    .unwrap();
    let applied = apply_transition(
        &mut state,
        "one",
        PartTransition::Retry {
            reason: "use another account".into(),
        },
    )
    .unwrap();
    assert_eq!(applied.previous, PartState::Failed);
    assert_eq!(applied.current, PartState::Pending);
}

#[test]
fn terminal_success_cannot_be_retried_or_overwritten() {
    let plan = plan(vec![part("one", Engine::Claude, &[], &[])]);
    let mut state = pending_state(&plan);
    apply_transition(&mut state, "one", PartTransition::Start).unwrap();
    apply_transition(&mut state, "one", PartTransition::Complete).unwrap();
    assert!(
        apply_transition(
            &mut state,
            "one",
            PartTransition::Retry {
                reason: "late result".into(),
            }
        )
        .is_err()
    );
    assert!(
        apply_transition(
            &mut state,
            "one",
            PartTransition::Fail {
                error: "late result".into(),
            }
        )
        .is_err()
    );
}
