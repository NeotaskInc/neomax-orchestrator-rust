use crate::Engine;
use crate::scheduler::{DependencyGraph, PartState};

use super::readiness::{DependencyReadiness, Readiness};
use super::test_support::{part, pending_state, plan};

#[test]
fn reports_ready_waiting_and_blocked_dependencies() {
    let plan = plan(vec![
        part("root", Engine::Claude, &[], &[]),
        part("child", Engine::Codex, &["root"], &[]),
        part("leaf", Engine::Kimi, &["child"], &[]),
    ]);
    let graph = plan.graph().unwrap();
    let mut state = pending_state(&plan);
    let readiness = DependencyReadiness::new(&graph, &state);
    assert_eq!(readiness.evaluate("root"), Readiness::Ready);
    assert_eq!(
        readiness.evaluate("child"),
        Readiness::Waiting {
            dependencies: vec!["root".into()]
        }
    );

    state.states.insert("root".into(), PartState::Failed);
    let readiness = DependencyReadiness::new(&graph, &state);
    assert_eq!(
        readiness.evaluate("child"),
        Readiness::Blocked {
            dependencies: vec!["root".into()]
        }
    );
    assert_eq!(readiness.evaluate("missing"), Readiness::UnknownPart);
}

#[test]
fn ready_order_preserves_dependency_priority() {
    let plan = plan(vec![
        part("root", Engine::Claude, &[], &[]),
        part("wide", Engine::Claude, &[], &[]),
        part("narrow", Engine::Claude, &[], &[]),
        part("child-a", Engine::Codex, &["wide"], &[]),
        part("child-b", Engine::Kimi, &["wide"], &[]),
    ]);
    let graph = DependencyGraph::build(&plan.parts).unwrap();
    let state = pending_state(&plan);
    let readiness = DependencyReadiness::new(&graph, &state);
    assert_eq!(readiness.ready_ids(), vec!["wide", "root", "narrow"]);
}
