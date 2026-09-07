use super::*;
use neomax_core::sessions::transcript::RecordedText;
use ratatui::widgets::Cell;

pub(super) fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if app.detail_open {
        detail(frame, app, area);
        return;
    }
    let parts = split(area);
    let sessions = app.filtered_sessions();
    let mut rows = sessions
        .iter()
        .map(|s| {
            let name = s
                .activity
                .as_ref()
                .and_then(|a| a.task.as_ref())
                .map(|t| t.text.as_str())
                .or(s.label.as_deref())
                .unwrap_or(&s.id);
            let title = clean(&format!(
                "{}{}",
                if s.is_child() { "↳ " } else { "" },
                name.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
            let owned = app.owned(s);
            Row::new(vec![
                Cell::from(ratatui::text::Text::from(vec![
                    Line::from(title),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{} / {}  ",
                                s.engine,
                                app.account_label(s.engine.as_str(), &s.account)
                            ),
                            Style::default().fg(DIM),
                        ),
                        Span::styled(
                            if owned { "Neomax" } else { "Native" },
                            Style::default().fg(if owned { VIOLET } else { DIM }),
                        ),
                    ]),
                ])),
                Cell::from(state(app, s).to_owned())
                    .style(Style::default().fg(state_color(state(app, s)))),
                Cell::from(short_date(
                    s.activity
                        .as_ref()
                        .and_then(|a| a.updated_at)
                        .or(s.last_active),
                )),
                Cell::from(tokens(s)),
            ])
            .height(2)
        })
        .collect::<Vec<_>>();
    rows.extend(app.unmatched_runs().iter().map(|r| {
        Row::new(vec![
            Cell::from(format!(
                "{}\n{} / {}  Neomax",
                clean(&r.prompt),
                r.engine,
                r.account
            )),
            Cell::from(r.status.clone()).style(Style::default().fg(state_color(&r.status))),
            Cell::from(short_date(r.ended.or(Some(r.started)))),
            Cell::from("—"),
        ])
        .height(2)
    }));
    render_table(
        frame,
        parts[0],
        &format!(
            "Sessions · {}d · {} · {}",
            app.fleet_days,
            rows.len(),
            ["All", "Recent activity", "Neomax"][app.fleet_view]
        ),
        &["Session", "State", "Last event", "Tokens"],
        rows,
        &[
            Constraint::Min(15),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(7),
        ],
        app.selected,
    );
    detail(frame, app, parts[1]);
}

pub(super) fn state<'a>(app: &'a App, s: &'a SessionRecord) -> &'a str {
    app.snapshot
        .runs
        .iter()
        .find(|r| r.engine == s.engine.as_str() && r.session.as_deref() == Some(&s.id))
        .map(|r| r.status.as_str())
        .unwrap_or_else(|| status(s))
}

fn state_color(value: &str) -> Color {
    match value {
        "recent" | "working" | "running" => TEAL,
        "failed" | "error" => Color::Rgb(232, 133, 133),
        "orphaned" | "blocked" => AMBER,
        _ => DIM,
    }
}

fn short_date(value: Option<i64>) -> String {
    value
        .and_then(|v| chrono::DateTime::from_timestamp(v, 0))
        .map(|v| {
            v.with_timezone(&chrono::Local)
                .format("%b %d\n%H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}

pub(super) fn date(value: Option<i64>) -> String {
    value
        .and_then(|v| chrono::DateTime::from_timestamp(v, 0))
        .map(|v| {
            v.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string()
        })
        .unwrap_or_else(|| "Not recorded".into())
}

fn section(lines: &mut Vec<Line<'static>>, label: &str) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        label.to_owned(),
        Style::default().fg(VIOLET).add_modifier(Modifier::BOLD),
    )));
}

fn text(lines: &mut Vec<Line<'static>>, value: &str) {
    lines.extend(clean(value).lines().map(|line| Line::from(line.to_owned())));
}

fn preview(
    lines: &mut Vec<Line<'static>>,
    value: Option<&RecordedText>,
    empty: &str,
    expanded: bool,
) {
    let Some(value) = value else {
        text(lines, empty);
        return;
    };
    if value.at.is_some() {
        lines.push(Line::from(Span::styled(
            date(value.at),
            Style::default().fg(DIM),
        )));
    }
    let limit = if expanded { usize::MAX } else { 360 };
    let displayed: String = value.text.chars().take(limit).collect();
    text(lines, &displayed);
    if value.truncated || value.text.chars().count() > limit {
        text(
            lines,
            if expanded {
                "[Display preview ends here; native transcript is unchanged.]"
            } else {
                "… Enter opens full details"
            },
        );
    }
}

fn detail(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(s) = app.selected_session() {
        lines.push(Line::from(vec![
            Span::styled(
                state(app, s).to_owned(),
                Style::default()
                    .fg(state_color(state(app, s)))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  {}  ", s.engine)),
            Span::styled(
                if app.owned(s) {
                    "Neomax-owned"
                } else {
                    "Native session"
                },
                Style::default().fg(VIOLET),
            ),
        ]));
        text(
            &mut lines,
            &app.account_label(s.engine.as_str(), &s.account),
        );
        text(
            &mut lines,
            s.model.as_deref().unwrap_or("Model not recorded"),
        );
        text(&mut lines, &format!("Started  {}", date(s.started)));
        let last = s.activity.as_ref().and_then(|a| a.updated_at);
        text(
            &mut lines,
            &format!(
                "{}  {}",
                if last.is_some() {
                    "Last event"
                } else {
                    "File updated"
                },
                date(last.or(s.last_active))
            ),
        );
        let activity = s.activity.as_ref();
        section(&mut lines, "Last assistant message");
        preview(
            &mut lines,
            activity.and_then(|a| a.last_message.as_ref()),
            "No assistant text found in the available transcript.",
            app.detail_open,
        );
        section(&mut lines, "Last tool call");
        if let Some(tool) = activity.and_then(|a| a.last_tool.as_ref()) {
            text(
                &mut lines,
                &format!(
                    "{} · {}",
                    tool.name,
                    tool.result
                        .as_deref()
                        .unwrap_or("no matching result recorded")
                ),
            );
            preview(
                &mut lines,
                Some(&tool.input),
                "Arguments not recorded",
                app.detail_open,
            );
        } else {
            text(
                &mut lines,
                "No tool invocation found in the available transcript.",
            );
        }
        section(&mut lines, "Task / first user message");
        if let Some(task) = activity.and_then(|a| a.task.as_ref()) {
            preview(&mut lines, Some(task), "", app.detail_open);
        } else {
            text(
                &mut lines,
                s.label
                    .as_deref()
                    .unwrap_or("Task text is not available from this session record."),
            );
        }
        if let Some(run) = app.selected_run() {
            section(&mut lines, "Managed run output");
            text(
                &mut lines,
                app.log
                    .as_ref()
                    .filter(|(id, _)| id == &run)
                    .map(|(_, text)| text.as_str())
                    .unwrap_or("Reading selected run output…"),
            );
        }
        section(&mut lines, "Recorded usage");
        text(
            &mut lines,
            &format!(
                "Input {}   Output {}\nCache read {}   Cache write {}",
                count(s.tokens.input),
                count(s.tokens.output),
                count(s.tokens.cache_read),
                count(s.tokens.cache_write)
            ),
        );
        if s.requests > 0 || s.tool_calls > 0 || s.errors > 0 {
            text(
                &mut lines,
                &format!(
                    "Reported counters: {} requests / {} tools / {} errors",
                    s.requests, s.tool_calls, s.errors
                ),
            );
        }
        if !s.files.is_empty() {
            section(&mut lines, "Files mentioned by tools");
            for file in &s.files {
                text(
                    &mut lines,
                    &format!("{}  +{} -{}", file.path, file.adds, file.dels),
                );
            }
        }
        section(&mut lines, "Session details");
        text(
            &mut lines,
            &format!(
                "Session {}\nParent {}",
                s.id,
                s.parent_id.as_deref().unwrap_or("Root session")
            ),
        );
        if let Some(cwd) = &s.cwd {
            text(&mut lines, &format!("Project {}", cwd.display()));
        }
        text(
            &mut lines,
            "Native status reflects transcript activity, not a verified live process. No model is called to build this view.",
        );
        if !app.owned(s) {
            text(
                &mut lines,
                "Neomax ownership is not recorded. This TUI cannot send input to a terminal opened elsewhere.",
            );
        }
        text(
            &mut lines,
            "Press c for the interactive orchestrator in Chat.",
        );
    } else if let Some(run) = app
        .unmatched_runs()
        .get(app.selected.saturating_sub(app.filtered_sessions().len()))
    {
        text(
            &mut lines,
            &format!(
                "{}  {} / {}  Neomax-owned\nStarted {}\nEnded {}",
                run.status,
                run.engine,
                run.account,
                date(Some(run.started)),
                date(run.ended)
            ),
        );
        section(&mut lines, "Task");
        text(&mut lines, &run.prompt);
        section(&mut lines, "Managed run output");
        text(
            &mut lines,
            app.log
                .as_ref()
                .filter(|(id, _)| id == &run.id)
                .map(|(_, text)| text.as_str())
                .unwrap_or("Reading selected run output…"),
        );
    } else {
        text(
            &mut lines,
            "No sessions match this view.\n\nThis is local provider history across all discovered accounts, including chats launched outside Neomax.\n\nv changes the view, d changes the date range, / searches. Press c to open Chat.",
        );
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(if app.detail_open {
                "Session details · ↑↓ scroll · Esc back"
            } else {
                "Selected session · Enter opens"
            }))
            .style(Style::default().fg(INK))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        area,
    );
}
