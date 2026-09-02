use anyhow::Result;

use super::types::StatusReport;

pub(crate) fn text(report: &StatusReport) -> Result<()> {
    println!("Neomax status at {}", report.now);
    println!(
        "fleet: accounts={}/{} running={} sessions={} subagents={} orchestrators={} queued={}",
        report.summary.accounts_up,
        report.summary.accounts_total,
        report.summary.running,
        report.summary.live_sessions,
        report.summary.subagents,
        report.summary.orchestrators,
        report.summary.queued_tasks
    );
    println!("providers: {}", report.connected_engines.join(", "));
    for provider in report.engines.values() {
        println!(
            "  {} binary={} available={} connected={} accounts={} models={} orchestrator={} workers={}",
            provider.engine,
            provider.binary,
            provider.binary_available,
            provider.connected,
            provider.accounts.len(),
            provider.available_models.len(),
            provider.orchestrator_eligible,
            provider.worker_eligible
        );
    }
    println!("accounts:");
    if report.accounts.is_empty() {
        println!("  none discovered");
    } else {
        for account in &report.accounts {
            let five_hour = percent(account.quota.five_hour_percent);
            let weekly = percent(account.quota.weekly_percent);
            let cooldown = account
                .quota
                .cooldown_until
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            let five_hour_reset = account
                .quota
                .five_hour_reset_at
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            let weekly_reset = account
                .quota
                .weekly_reset_at
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            let methods = if account.auth_methods.is_empty() {
                "-".to_owned()
            } else {
                account.auth_methods.join(",")
            };
            println!(
                "  {:<9} {:<12} role={} auth={} methods={} live={} workers={} subagents={} 5h={} 7d={} reset5h={} reset7d={} cooldown={} paused={} hard_wall={}",
                account.engine,
                account.identity,
                account.role,
                account.auth_status,
                methods,
                account.live,
                account.live_workers,
                account.subagents,
                five_hour,
                weekly,
                five_hour_reset,
                weekly_reset,
                cooldown,
                account.paused,
                account.quota.hard_wall
            );
        }
    }
    println!("sessions:");
    if report.sessions.is_empty() {
        println!("  none");
    } else {
        for session in &report.sessions {
            println!(
                "  {} run={} {}:{} model={} status={} children={}",
                session.id,
                session.run_id,
                session.engine,
                session.account,
                session.model,
                session.status,
                session.child_count
            );
        }
    }
    println!("ambient sessions: {}", report.ambient.len());
    for session in report.ambient.iter().filter(|session| session.active) {
        println!(
            "  {} {}:{} model={} working={} children={}",
            session.id,
            session.engine,
            session.account,
            session.model.as_deref().unwrap_or("-"),
            session.working,
            usize::from(session.kind != neomax_core::sessions::SessionKind::Main)
        );
    }
    println!("subagents: {}", report.subagents.len());
    for subagent in &report.subagents {
        println!(
            "  {} run={} {}:{} status={}{}",
            subagent.id,
            subagent.run_id,
            subagent.engine,
            subagent.account,
            subagent.status,
            subagent
                .label
                .as_deref()
                .map_or(String::new(), |label| format!(" label={label}"))
        );
    }
    println!("orchestrators: {}", report.orchestrators.len());
    if report.orchestrators.is_empty() {
        println!("  none");
    } else {
        for orchestrator in &report.orchestrators {
            println!(
                "  {} {}:{} model={} live={} pid={}",
                orchestrator.session,
                orchestrator.engine,
                orchestrator.account,
                orchestrator.model,
                orchestrator.live,
                orchestrator
                    .pid
                    .map_or_else(|| "-".into(), |pid| pid.to_string())
            );
        }
    }
    println!("runs: {}", report.runs.len());
    for run in &report.runs {
        println!(
            "  {:<24} {:<11} {:<8} account={} model={} session={} children={}",
            run.id,
            run.status,
            run.engine,
            run.account,
            run.model,
            run.session.as_deref().unwrap_or("-"),
            run.child_count
        );
    }
    println!(
        "queue: used={} free={} active={} queued={} agent_budget={} task_budget={}",
        report.queue.used,
        report.queue.free,
        report.queue.active_tasks,
        report.queue.queued_tasks,
        report.queue.agent_budget,
        report.queue.task_budget
    );
    Ok(())
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.1}%"))
}
