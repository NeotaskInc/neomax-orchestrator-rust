use super::*;

#[test]
fn normalizes_universal_account_selector_forms() {
    let relative_profile = std::path::PathBuf::from("profiles")
        .join("..")
        .join("profiles")
        .join("current");
    let literal_profile = std::path::PathBuf::from("fixture").join("literal-profile");
    let selectors = parse_account_selectors(&[
        "account".to_owned(),
        "2".to_owned(),
        "acct-3".to_owned(),
        "one".to_owned(),
        "to".to_owned(),
        "ORCH".to_owned(),
        relative_profile.to_string_lossy().into_owned(),
        "acct_4".to_owned(),
        "account5".to_owned(),
    ]);
    assert_eq!(selectors[0], AccountSelector::Number("2".into()));
    assert_eq!(selectors[1], AccountSelector::Number("3".into()));
    assert_eq!(selectors[2], AccountSelector::Number("1".into()));
    assert_eq!(selectors[3], AccountSelector::Orchestrator);
    assert_eq!(
        selectors[4],
        AccountSelector::Profile(normalize_profile_path(&relative_profile))
    );
    assert_eq!(selectors[5], AccountSelector::Number("4".into()));
    assert_eq!(selectors[6], AccountSelector::Number("5".into()));
    assert_eq!(
        parse_account_selector(literal_profile.to_string_lossy().as_ref())
            .unwrap()
            .profile(),
        Some(normalize_profile_path(&literal_profile).as_path())
    );
}

#[test]
fn expands_home_and_normalizes_parent_segments() {
    let root = std::env::temp_dir().join("neomax-selector-fixture");
    let home = root.join("home");
    let current_dir = root.join("project");
    let runtime = crate::runtime::RuntimeEnvironment::fixture(
        crate::runtime::RuntimePlatform::current(),
        [("HOME".into(), home.to_string_lossy().into_owned())],
        current_dir,
    );
    let expected = home.join("active");
    assert_eq!(
        super::selectors::normalize_profile_path_with_environment("~/profiles/../active", &runtime,),
        expected
    );
    let traversing = root.join("tmp").join("a").join("..").join("..").join("b");
    assert_eq!(
        super::selectors::normalize_profile_path_with_environment(traversing, &runtime,),
        root.join("b")
    );
}

#[cfg(windows)]
#[test]
fn rejects_windows_partial_root_profile_selectors_without_rehoming_them() {
    let root = std::env::temp_dir().join("neomax-selector-fixture");
    let runtime = crate::runtime::RuntimeEnvironment::fixture(
        crate::runtime::RuntimePlatform::Windows,
        [],
        root.join("project"),
    );

    for raw in [r"\rooted", r"C:drive-relative"] {
        assert!(parse_account_selector(raw).is_none());
        assert_eq!(
            super::selectors::normalize_profile_path_with_environment(raw, &runtime),
            std::path::PathBuf::from(raw)
        );
    }
}
