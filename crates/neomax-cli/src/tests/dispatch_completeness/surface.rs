use super::aliases::MULTICALL_LAUNCHERS;
use crate::cli;
use crate::tests::fixture;

#[test]
fn every_orchestrator_alias_exposes_the_shared_portal_surface() {
    let fixture = fixture();
    for (alias, launcher) in MULTICALL_LAUNCHERS.iter().filter(|(_, launcher)| {
        matches!(
            launcher,
            neomax_core::orchestration::commands::Launcher::ProviderOrchestrator(_)
        )
    }) {
        let help = cli::help_text(*launcher);
        for surface in [
            "portal",
            "orient",
            "usage-watch",
            "keepalive",
            "turn-hook",
            "model-guard",
            "usage-hook",
        ] {
            assert!(
                help.contains(surface),
                "{alias} help must advertise the shared {surface} command"
            );
        }

        let status = cli::execute(
            *launcher,
            &["status".into(), "--json".into()],
            &fixture.context,
        );
        assert_no_dispatch_gap(status, alias, "status");

        let usage = cli::execute(
            *launcher,
            &["usage".into(), "--json".into()],
            &fixture.context,
        );
        assert_no_dispatch_gap(usage, alias, "usage");

        let rotation = cli::execute(
            *launcher,
            &["rotate-tick".into(), "--json".into()],
            &fixture.context,
        );
        assert_no_dispatch_gap(rotation, alias, "rotate-tick");
    }
}

fn assert_no_dispatch_gap(result: anyhow::Result<()>, launcher: &str, command: &str) {
    if let Err(error) = result {
        assert!(
            !super::handlers::is_dispatch_gap(&error.to_string()),
            "{launcher} {command} is not wired: {error:#}"
        );
    }
}
