use super::query::{UsageRange, collect};

#[test]
fn parses_all_supported_time_windows() {
    assert_eq!(super::query::parse_duration("90s").unwrap(), 90);
    assert_eq!(super::query::parse_duration("37m").unwrap(), 37 * 60);
    assert_eq!(super::query::parse_duration("2h").unwrap(), 2 * 3_600);
    assert_eq!(super::query::parse_duration("3d").unwrap(), 3 * 86_400);
    assert_eq!(super::query::parse_duration("1w").unwrap(), 604_800);
}

#[test]
fn rejects_ambiguous_or_malformed_ranges() {
    assert!(UsageRange::parse(&["--days".into(), "2".into(), "--all".into()]).is_err());
    assert!(UsageRange::parse(&["--since".into(), "37".into()]).is_err());
    assert!(UsageRange::parse(&["--days".into(), "0".into()]).is_err());
    assert_eq!(
        UsageRange::parse(&["--all".into(), "--json".into()]).unwrap(),
        UsageRange::All
    );
}

#[test]
fn since_window_reports_fleet_dimensions_from_the_local_ledger() {
    use crate::tests::fixture;
    use neomax_core::Engine;
    use neomax_core::usage::{LedgerKind, LedgerRecord, UsageLedger};

    let fixture = fixture();
    let timestamp = fixture.context.now - 120;
    UsageLedger::new(&fixture.context.paths.usage_ledger)
        .append(&[
            LedgerRecord {
                ts: timestamp,
                engine: Engine::Kimi,
                account: "account-2".into(),
                model: "kimi-code/k3".into(),
                id: "usage-1".into(),
                kind: LedgerKind::Add,
                session: Some("session-1".into()),
                agent: Some("agent-1".into()),
                input: 10,
                output: 20,
                reasoning: 3,
                cache_write: 0,
                cache_read: 0,
                cost: None,
                requests: Some(1),
                completions: Some(1),
                errors: 0,
                rate_limits: 0,
                extra: Default::default(),
            },
            LedgerRecord {
                ts: fixture.context.now - 400,
                engine: Engine::Kimi,
                account: "account-2".into(),
                model: "kimi-code/k3".into(),
                id: "usage-old".into(),
                kind: LedgerKind::Add,
                session: Some("session-old".into()),
                agent: Some("agent-old".into()),
                input: 100,
                output: 100,
                reasoning: 0,
                cache_write: 0,
                cache_read: 0,
                cost: None,
                requests: Some(1),
                completions: Some(1),
                errors: 0,
                rate_limits: 0,
                extra: Default::default(),
            },
        ])
        .unwrap();
    let (report, range) = collect(&fixture.context, &["--since".into(), "5m".into()]).unwrap();
    assert_eq!(range, UsageRange::Since { seconds: 300 });
    assert_eq!(report.report.grand.output, 20);
    assert_eq!(report.report.by_account.len(), 1);
    assert_eq!(report.report.by_model.len(), 1);
    assert_eq!(report.report.by_session.len(), 1);
    assert_eq!(report.report.by_agent.len(), 1);

    let (all, range) = collect(&fixture.context, &["--all".into()]).unwrap();
    assert_eq!(range, UsageRange::All);
    assert_eq!(all.report.grand.output, 120);
}
