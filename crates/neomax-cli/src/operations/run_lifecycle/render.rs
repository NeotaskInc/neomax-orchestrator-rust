use std::fmt::Write;

use super::{RunLifecycleReport, RunView};

pub(crate) fn text(report: &RunLifecycleReport) -> String {
    match report {
        RunLifecycleReport::List(report) => list(report),
        RunLifecycleReport::Log(report) => log(report),
        RunLifecycleReport::Rerun(run) => format_run(run),
        RunLifecycleReport::Kill(report) => format!(
            "{} {}: {}{}",
            report.id,
            report.status,
            report.message,
            if report.worktree_preserved {
                "; worktree preserved for resume"
            } else {
                ""
            }
        ),
        RunLifecycleReport::History(report) => history(report),
        RunLifecycleReport::Status(report) => status(report),
        RunLifecycleReport::Diff(report) => diff(report),
        RunLifecycleReport::SubagentDiff(report) => subagent_diff(report),
    }
}

fn list(report: &super::listing::RunListReport) -> String {
    if report.runs.is_empty() {
        return "no neomax runs recorded".into();
    }
    let mut output = String::from("RUN STATUS ENGINE/ACCOUNT AGE BRANCH PROMPT\n");
    for run in &report.runs {
        let age = run.started;
        let _ = writeln!(
            output,
            "{} {} {}/{} {} {} {}",
            run.id,
            run.status,
            run.engine,
            run.account,
            age,
            run.branch.as_deref().unwrap_or("-"),
            run.prompt.replace('\n', " "),
        );
    }
    let _ = writeln!(
        output,
        "inbox={} orphaned={}",
        report.inbox, report.orphaned
    );
    output
}

fn log(report: &super::logs::LogReport) -> String {
    let mut output = String::new();
    for entry in &report.entries {
        match entry {
            super::logs::LogEntry::Text { text } => {
                let _ = writeln!(output, "· {text}");
            }
            super::logs::LogEntry::Tool { name, input } => {
                let _ = writeln!(output, "⚙ {name} {input}");
            }
            super::logs::LogEntry::Result { subtype, text } => {
                let _ = writeln!(
                    output,
                    "== result ({}) ==\n{text}",
                    subtype.as_deref().unwrap_or("unknown")
                );
            }
            super::logs::LogEntry::Event { event_type, raw } => {
                let _ = writeln!(output, "[{event_type:?}] {raw}");
            }
        }
    }
    if report.truncated {
        output.push_str("[log tail truncated]\n");
    }
    output
}

fn format_run(run: &RunView) -> String {
    format!(
        "{} {} attempt={} engine={} account={} model={}",
        run.id, run.status, run.attempt, run.engine, run.account, run.model
    )
}

fn history(report: &super::history::HistoryReport) -> String {
    if let Some(detail) = report.detail.as_ref() {
        let mut output = format_run(&detail.run);
        let _ = writeln!(output, "\narchived_status={}", detail.archived_status);
        if let Some(path) = detail.log_path.as_deref() {
            let _ = writeln!(output, "log={path}");
        }
        if let Some(log) = report.log.as_ref() {
            output.push_str(&self::log(log));
        }
        return output;
    }
    if report.rows.is_empty() {
        return "no run history yet".into();
    }
    let mut output = String::from("RUN ENGINE ACCOUNT STATUS SUB STARTED PROMPT\n");
    for row in &report.rows {
        let _ = writeln!(
            output,
            "{} {} {} {} {} {} {}",
            row.id,
            row.engine,
            row.account,
            row.status.as_str(),
            row.children,
            row.ended.unwrap_or(row.started),
            row.prompt.as_deref().unwrap_or_default(),
        );
    }
    output
}

fn status(report: &super::listing::RunStatusReport) -> String {
    format!(
        "Neomax status at {}\nruns={} running={} orphaned={} inbox={}",
        report.now,
        report.runs.len(),
        report.running,
        report.orphaned,
        report.inbox
    )
}

fn diff(report: &super::diff::DiffReport) -> String {
    let mut output = format!(
        "diff {} ({} vs {}) - {} files\n",
        report.branch,
        report.id,
        report.base,
        report.files.len()
    );
    for file in &report.files {
        let _ = writeln!(output, "  +{} -{} {}", file.adds, file.dels, file.path);
    }
    if let Some(patch) = report.patch.as_deref() {
        output.push_str(patch);
        if report.patch_truncated {
            output.push_str("\n[patch truncated]\n");
        }
    }
    output
}

fn subagent_diff(report: &super::diff::SubagentDiffReport) -> String {
    let mut output = format!(
        "subagent-diff {} - {} edits, {} files\n",
        report.id,
        report.edits,
        report.files.len()
    );
    for file in &report.files {
        let _ = writeln!(output, "  +{} -{} {}", file.adds, file.dels, file.path);
        if let Some(patch) = file.patch.as_deref() {
            let _ = writeln!(output, "{patch}");
        }
    }
    output
}
