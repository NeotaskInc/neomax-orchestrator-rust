use super::*;

#[test]
fn parses_copy_swap_restore_and_log_as_distinct_operations() {
    let copy = AuthOptions::parse(&[
        "2".into(),
        "--from".into(),
        "1".into(),
        "--engine".into(),
        "claude".into(),
    ])
    .unwrap();
    assert_eq!(copy.destination.as_deref(), Some("2"));
    assert_eq!(copy.source.as_deref(), Some("1"));
    assert!(!copy.swap);

    let swap = AuthOptions::parse(&["--destination=2".into(), "--from=1".into(), "--swap".into()])
        .unwrap();
    assert!(swap.swap);

    let restore = AuthOptions::parse(&["--restore".into(), "2".into()]).unwrap();
    assert_eq!(restore.restore.as_deref(), Some("2"));

    let log = AuthOptions::parse(&["--log".into(), "--json".into()]).unwrap();
    assert!(log.log && log.json);
}

#[test]
fn no_mutation_request_is_safe_without_provider_discovery() {
    let fixture = crate::tests::fixture();
    execute(Launcher::Universal, &["--json".into()], &fixture.context).unwrap();
}
