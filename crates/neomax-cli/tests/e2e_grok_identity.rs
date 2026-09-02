#[path = "e2e_support/mod.rs"]
mod support;

use std::fs;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn gmx_whoami_reads_safe_local_identity_metadata_without_provider_execution() {
    let harness = E2eHarness::new([Engine::Grok]);
    let profile = harness.profile(Engine::Grok, 0);
    fs::write(
        profile.join("auth.json"),
        br#"{"xai::oidc":{"auth_mode":"oidc","key":"raw-api-token","email":"person@example.test","first_name":"Ada","last_name":"Lovelace","team_name":"Analytical Engine"}}"#,
    )
    .unwrap();

    let result = harness.run_alias("gmx", ["--json", "whoami", "1"]);
    let report = result.json();
    assert_eq!(report["operation"], "whoami");
    assert_eq!(report["engine"], "grok");
    assert_eq!(report["success"], true);
    assert!(report["stdout"].as_str().unwrap().contains("method: OAuth"));
    assert!(
        report["stdout"]
            .as_str()
            .unwrap()
            .contains("email: person@example.test")
    );
    assert!(
        report["stdout"]
            .as_str()
            .unwrap()
            .contains("name: Ada Lovelace")
    );
    assert!(
        report["stdout"]
            .as_str()
            .unwrap()
            .contains("team: Analytical Engine")
    );
    assert!(!result.stdout.contains("raw-api-token"));
    assert!(harness.invocations().is_empty());
    harness.assert_hermetic_invocations();
}
