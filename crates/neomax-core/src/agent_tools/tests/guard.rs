use crate::agent_tools::{DEFAULT_MAX_DEPTH, RecursionGuard};

#[test]
fn guard_advances_until_the_limit() {
    let root = RecursionGuard::new(0, 2).unwrap();
    let child = root.enter().unwrap();
    assert_eq!(child.depth(), 1);
    let grandchild = child.enter().unwrap();
    assert_eq!(grandchild.depth(), 2);
    assert!(grandchild.enter().is_err());
}

#[test]
fn guard_reads_and_validates_environment_values() {
    let guard = RecursionGuard::from_environment(Some("2"), Some("3")).unwrap();
    assert_eq!(guard.depth(), 2);
    assert_eq!(guard.max_depth(), 3);
    assert_eq!(
        RecursionGuard::from_environment(None, None)
            .unwrap()
            .max_depth(),
        DEFAULT_MAX_DEPTH
    );
    assert!(RecursionGuard::from_environment(Some("bad"), None).is_err());
    assert!(RecursionGuard::from_environment(Some("4"), Some("3")).is_err());
}
