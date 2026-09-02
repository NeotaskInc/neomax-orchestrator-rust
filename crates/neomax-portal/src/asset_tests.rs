const PORTAL_CSS: &str = include_str!("../assets/styles.css");
const PORTAL_HTML: &str = include_str!("../assets/index.html");
const PORTAL_RENDER: &str = include_str!("../assets/render.js");
const PORTAL_DELIVERY: &str = include_str!("../assets/delivery.js");

#[test]
fn mobile_layout_contract_keeps_fleet_cards_inside_the_viewport() {
    for rule in [
        "@media (max-width: 560px)",
        ".topbar > div:first-child { flex-basis: 100%; }",
        ".engine-grid { grid-template-columns: minmax(0, 1fr); }",
        ".engine-head .hint { flex-basis: 100%; text-align: left; }",
        ".account { grid-template-columns: 9px minmax(0, 1fr); }",
        ".account > * { min-width: 0; }",
        "overflow-wrap: anywhere",
        ".table-scroll { overflow-x: auto; }",
    ] {
        assert!(
            PORTAL_CSS.contains(rule),
            "missing responsive layout rule: {rule}"
        );
    }
    assert!(
        !PORTAL_CSS.contains("overflow-x: hidden"),
        "global overflow clipping must not mask layout regressions"
    );
}

#[test]
fn universal_dashboard_keeps_all_provider_and_usage_surfaces_visible() {
    for provider in ["Claude", "Codex", "OpenCode", "Kimi", "Grok"] {
        assert!(
            PORTAL_HTML.contains(provider),
            "missing provider label: {provider}"
        );
    }
    for surface in [
        "id=\"rotation-banner\"",
        "id=\"rotation-history\"",
        "id=\"rotations\"",
        "id=\"ambient\"",
        "id=\"history\"",
        "id=\"sessions\"",
        "id=\"subagents\"",
        "id=\"usage\"",
        "id=\"usage-breakdowns\"",
        "id=\"projects\"",
        "data-tab=\"delivery\"",
        "id=\"plans\"",
        "id=\"issues\"",
        "id=\"worktrees\"",
        "id=\"failovers\"",
    ] {
        assert!(
            PORTAL_HTML.contains(surface),
            "missing portal surface: {surface}"
        );
    }
    for field in [
        "renderRotationHistory",
        "authentication rotation",
        "last six hours",
        "reactive telemetry",
        "rate limits",
        "completions",
        "cache read",
        "cache write",
        "native subagents",
        "rotation",
        "managed_pool_eligible",
        "OpenCode SQLite detail",
        "Kimi local detail",
        "Grok local detail",
        "unfinished",
        "tool_usage",
        "esc(detail.account",
        "esc(detail.source",
        "esc(row.tool",
        "esc(row.status",
        "esc(detail.error",
    ] {
        assert!(
            PORTAL_RENDER.contains(field),
            "missing rendered field: {field}"
        );
    }
    assert!(
        PORTAL_DELIVERY.contains("renderFailovers"),
        "missing failover event renderer"
    );
    let em_dash = char::from_u32(0x2014).expect("valid em dash code point");
    assert!(!PORTAL_HTML.contains(em_dash));
    assert!(!PORTAL_RENDER.contains(em_dash));
    assert!(!PORTAL_DELIVERY.contains(em_dash));
}
