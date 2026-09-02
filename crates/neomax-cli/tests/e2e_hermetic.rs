#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::E2eHarness;

#[test]
fn missing_binary_overrides_fail_into_fixtures_before_any_host_provider() {
    for engine in Engine::ALL {
        let harness = E2eHarness::new([engine]);
        let mut args = vec![
            "--json".to_owned(),
            "--foreground".to_owned(),
            "--engine".to_owned(),
            engine.as_str().to_owned(),
        ];
        if engine != Engine::Kimi {
            args.push("fixture task".into());
        }

        let result = harness.run_without_binary_override(engine, args);
        let report = result.json();
        assert_eq!(report["status"], "done", "engine {engine}: {report}");
        assert!(
            !harness.invocations().is_empty(),
            "engine {engine} did not reach its fixture provider"
        );
        harness.assert_hermetic_invocations();
    }
}
