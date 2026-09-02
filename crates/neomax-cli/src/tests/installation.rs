use crate::installation;

#[test]
fn validates_installation_flags_and_values() {
    installation::validate_flags(&[
        "--package-root".into(),
        "/tmp/package".into(),
        "--install-root=/tmp/install".into(),
        "--force".into(),
        "--json".into(),
        "--no-usage-agent".into(),
    ])
    .expect("valid installation flags");
}

#[test]
fn rejects_missing_or_unknown_installation_flags() {
    let missing = installation::validate_flags(&["--package-root".into()])
        .expect_err("missing package root should fail");
    assert!(
        missing
            .to_string()
            .contains("--package-root requires a value")
    );

    let option_as_value =
        installation::validate_flags(&["--install-root".into(), "--force".into()])
            .expect_err("option must not be accepted as a path");
    assert!(
        option_as_value
            .to_string()
            .contains("--install-root requires a value")
    );

    let unknown = installation::validate_flags(&["--provider".into()])
        .expect_err("unknown installation option should fail");
    assert!(unknown.to_string().contains("unknown installation option"));
}
