use anyhow::Result;

use super::query::UsageOutput;

pub(crate) fn text(output: &UsageOutput) -> Result<()> {
    let report = &output.report;
    println!(
        "usage ({}): input={} output={} reasoning={} requests={} completions={} errors={} rate_limits={} cost=${:.2}",
        output.range,
        report.grand.input,
        report.grand.output,
        report.grand.reasoning,
        report.grand.requests,
        report.grand.completions,
        report.grand.errors,
        report.grand.rate_limits,
        report.grand.cost
    );
    print_provider_rows(report);
    print_account_rows(report);
    print_model_rows(report);
    print_session_rows(report);
    print_agent_rows(report);
    Ok(())
}

fn print_provider_rows(report: &neomax_core::usage::UsageReport) {
    println!("by provider:");
    if report.by_provider.is_empty() {
        println!("  none");
    }
    for row in &report.by_provider {
        println!(
            "  {:<10} input={} output={} reasoning={} requests={} cost=${:.2}",
            row.provider,
            row.metrics.input,
            row.metrics.output,
            row.metrics.reasoning,
            row.metrics.requests,
            row.metrics.cost
        );
    }
}

fn print_account_rows(report: &neomax_core::usage::UsageReport) {
    println!("by account:");
    for row in &report.by_account {
        println!(
            "  {}:{} input={} output={} requests={} cost=${:.2}",
            row.provider,
            row.account,
            row.metrics.input,
            row.metrics.output,
            row.metrics.requests,
            row.metrics.cost
        );
    }
}

fn print_model_rows(report: &neomax_core::usage::UsageReport) {
    println!("by model:");
    for row in &report.by_model {
        println!(
            "  {}/{} input={} output={} requests={} cost=${:.2}",
            row.provider,
            row.model,
            row.metrics.input,
            row.metrics.output,
            row.metrics.requests,
            row.metrics.cost
        );
    }
}

fn print_session_rows(report: &neomax_core::usage::UsageReport) {
    println!("by session:");
    for row in &report.by_session {
        println!(
            "  {}:{} input={} output={} requests={} cost=${:.2}",
            row.provider,
            row.session,
            row.metrics.input,
            row.metrics.output,
            row.metrics.requests,
            row.metrics.cost
        );
    }
}

fn print_agent_rows(report: &neomax_core::usage::UsageReport) {
    println!("by agent:");
    for row in &report.by_agent {
        println!(
            "  {}:{}:{} input={} output={} requests={} cost=${:.2}",
            row.provider,
            row.account,
            row.agent,
            row.metrics.input,
            row.metrics.output,
            row.metrics.requests,
            row.metrics.cost
        );
    }
}
