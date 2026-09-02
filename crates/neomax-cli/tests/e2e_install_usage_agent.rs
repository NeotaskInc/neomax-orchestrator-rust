#[cfg(unix)]
#[path = "e2e_install_support/mod.rs"]
mod support;

#[cfg(unix)]
#[test]
fn json_install_remains_machine_readable_when_usage_agent_writes_output() {
    let fixture = support::InstallFixture::new();
    fixture.materialize_package();

    let install = fixture.run_with_usage_agent(&fixture.paths_args("install"));
    assert!(
        install.status.success(),
        "install failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&install.stdout),
        String::from_utf8_lossy(&install.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&install.stdout)
        .expect("usage-agent output must not pollute install JSON");
    assert_eq!(report["product"], "neomax");

    let uninstall = fixture.run_with_usage_agent(&fixture.paths_args("uninstall"));
    assert!(uninstall.status.success());
    serde_json::from_slice::<serde_json::Value>(&uninstall.stdout)
        .expect("usage-agent output must not pollute uninstall JSON");
}
