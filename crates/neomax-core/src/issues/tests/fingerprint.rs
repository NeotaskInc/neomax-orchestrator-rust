use crate::issues::issue_fingerprint;

#[test]
fn fingerprints_normalize_titles_and_repository_order() {
    let a = issue_fingerprint(
        "Race in proxy worker pool",
        Some("demo"),
        &["b".into(), "a".into()],
    );
    let b = issue_fingerprint(
        "race  in   PROXY worker pool!!",
        Some("demo"),
        &["a".into(), "b".into()],
    );
    assert_eq!(a, b);
}
