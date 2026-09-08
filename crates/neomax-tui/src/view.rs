use std::path::Path;

#[path = "view/accounts.rs"]
mod accounts_view;
#[path = "view/fleet.rs"]
mod fleet_view;

use neomax_core::sessions::SessionRecord;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState, Wrap},
};
use serde_json::Value;

use crate::{
    app::{ACCOUNTS, App, CHAT, Confirm, FLEET, LAUNCH, PAGES, TASKS, USAGE},
    data::clean,
};

const INK: Color = Color::Reset;
const DIM: Color = Color::Rgb(119, 133, 151);
const VIOLET: Color = Color::Rgb(180, 160, 229);
const TEAL: Color = Color::Rgb(118, 204, 193);
const AMBER: Color = Color::Rgb(221, 187, 111);
const LOGO: &str = "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣠⣾⠀\n⠀⠀⠀⠀⣀⡀⠀⠀⣠⣾⣿⡆⠀⠀\n⠀⠀⠀⢸⡿⢿⣶⣾⠟⢽⡿⠁⠀⠀\n⣠⣴⣶⠾⢿⣿⡿⣿⣿⠿⠷⣶⣦⣄\n⠙⠻⠿⢦⣾⡟⠁⠈⢿⣧⠰⠿⠟⠋\n⠀⠀⢠⣿⣟⣤⡄⠠⣼⣿⡇⠀⠀⠀\n⠀⠀⠸⣿⠿⠋⠀⠀⠈⠉⠀⠀⠀⠀";

fn panel(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(DIM))
        .title(Line::from(Span::styled(
            format!(" {} ", title.into()),
            Style::default().fg(INK),
        )))
}

fn spend_header(frame: &mut Frame, app: &App, area: Rect) {
    let tabs = ["24h", "7d", "30d"]
        .iter()
        .enumerate()
        .map(|(index, label)| {
            Span::styled(
                format!(" {label} "),
                if index == app.spend_range {
                    Style::default().fg(Color::Black).bg(TEAL)
                } else {
                    Style::default().fg(DIM)
                },
            )
        })
        .collect::<Vec<_>>();
    let mut lines = vec![Line::from(tabs)];
    if let Some(report) = app
        .spend
        .as_ref()
        .filter(|report| report.days == app.spend_days())
    {
        lines.push(Line::from(Span::styled(
            format!("${:.2}", report.grand.cost),
            Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            if report.warnings.is_empty() {
                "Local API estimate"
            } else {
                "Partial local estimate"
            },
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(format!(
            "{} in  {} out",
            count(report.grand.input),
            count(report.grand.output)
        )));
        lines.push(Line::from(format!(
            "Cache {} r {} w",
            count(report.grand.cache_read),
            count(report.grand.cache_write)
        )));
        lines.push(Line::from(Span::styled(
            format!("{}d · all providers", report.days),
            Style::default().fg(DIM),
        )));
        let age = chrono::Utc::now()
            .timestamp()
            .saturating_sub(report.now)
            .max(0);
        lines.push(Line::from(Span::styled(
            if app.usage_error.is_some() {
                format!("Cached {age}s · refresh failed")
            } else {
                format!("Updated {age}s · s range")
            },
            Style::default().fg(if app.usage_error.is_some() {
                AMBER
            } else {
                DIM
            }),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Loading usage…",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from("Local API estimate"));
        lines.push(Line::from("s changes range"));
        if app.usage_error.is_some() {
            lines.push(Line::from(Span::styled(
                "Usage unavailable",
                Style::default().fg(AMBER),
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn body(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec()
}

pub fn terminal_area(area: Rect) -> Rect {
    panel("").inner(body(area)[2])
}

pub fn draw(frame: &mut Frame, app: &App, cwd: &Path) {
    let area = frame.area();
    if area.width < 60 || area.height < 20 {
        frame.render_widget(Paragraph::new("Neomax\n\nResize to at least 60 × 20.\nq exits; an open orchestrator requires confirmation.").wrap(Wrap { trim: false }), area);
        if app.confirmation.is_some() {
            popup(
                frame,
                "Stop orchestrator and quit?",
                "y confirms · any other key cancels",
            );
        }
        return;
    }
    let parts = body(area);
    let header = Layout::horizontal([
        Constraint::Length(16),
        Constraint::Min(20),
        Constraint::Length(if area.width >= 90 { 29 } else { 23 }),
    ])
    .split(parts[0]);
    frame.render_widget(
        Paragraph::new(LOGO).style(Style::default().fg(INK)),
        header[0],
    );
    let age = app
        .refreshed
        .map(|t| format!("updated {}s ago", t.elapsed().as_secs()))
        .unwrap_or_else(|| "loading local state".into());
    let active = app
        .sessions
        .iter()
        .filter(|s| s.active || s.working)
        .count();
    let child = app
        .sessions
        .iter()
        .filter(|s| s.is_child() && (s.active || s.working))
        .count();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "N E O M A X",
                Style::default().fg(VIOLET).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "NEOTASK  /  AGENT WORKSPACE",
                Style::default().fg(DIM),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("{active} recent"), Style::default().fg(TEAL)),
                Span::raw(format!(
                    "  {child} subagents  {} Neomax tasks",
                    app.snapshot.summary.tasks_open
                )),
            ]),
            Line::from(clean(&cwd.display().to_string())),
            Line::from(Span::styled(age, Style::default().fg(DIM))),
        ])
        .style(Style::default().fg(INK)),
        header[1],
    );
    spend_header(frame, app, header[2]);
    let tabs = PAGES
        .iter()
        .enumerate()
        .flat_map(|(i, name)| {
            [
                Span::styled(
                    format!(" {name} "),
                    if i == app.page {
                        Style::default()
                            .fg(Color::Black)
                            .bg(TEAL)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(DIM)
                    },
                ),
                Span::raw(" "),
            ]
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Line::from(tabs)), parts[1]);
    match app.page {
        FLEET => fleet(frame, app, parts[2]),
        TASKS => tasks(frame, app, parts[2]),
        USAGE => usage(frame, app, parts[2]),
        LAUNCH => launch(frame, app, parts[2]),
        ACCOUNTS => accounts_view::draw(frame, app, parts[2]),
        _ => orchestrator(frame, app, parts[2]),
    }
    let footer = if !app.message.is_empty() {
        clean(&app.message)
    } else if app.searching {
        format!("Find: {}  [Enter done · Esc clear]", clean(&app.filter))
    } else if app.page == CHAT && app.terminal.as_ref().is_some_and(|t| t.input) {
        "INPUT → orchestrator   Ctrl+] returns to dashboard controls".into()
    } else if app.page == LAUNCH && app.launch.editing {
        "Edit prompt · Enter newline · Esc done".into()
    } else if app.page == LAUNCH && app.launch.choosing {
        "Edit choice · ←→ changes value · Enter or Esc done".into()
    } else if app.detail_open {
        "↑↓ / PgUp/PgDn scroll · Esc back · ←→ pages · s spend range".into()
    } else if app.page == FLEET {
        "←→ pages · ↑↓ select · Enter details · v view · d days · / find · s spend · c chat".into()
    } else {
        "←→ pages · ↑↓ select · Enter open/edit · s spend · r refresh · ? keys · q quit".into()
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(if app.message.is_empty() {
            DIM
        } else {
            AMBER
        })),
        parts[3],
    );
    if app.help {
        popup(
            frame,
            "Keyboard",
            "←→ / Tab     change page\n↑↓ / j k     select row or launch field\nEnter        open details / edit choice / confirm launch\n←→ then Enter change and save an open choice\n[ ]          filter Accounts by provider\nl            launch with selected account\n/            filter Fleet; Esc clears\nv / d        Fleet view / date range\nd            Usage date range\ns            header spend range: 24h / 7d / 30d\nPgUp/PgDn    scroll detail or terminal history\nCtrl+]       leave Chat input focus\nr            refresh local state\nq            quit (asks before stopping owned orchestrator)\n\nExisting sessions are view-only. Unknown telemetry stays unknown.\nPress any key to close.",
        );
    }
    if let Some(confirm) = app.confirmation {
        match confirm {
            Confirm::Launch => popup(
                frame,
                "Start orchestrator?",
                &format!(
                    "ENTER TO START  ·  Esc to cancel\n\nThis opens the provider in Chat and may use your account allowance.\n\n{}",
                    app.launch_args()
                        .map(|a| format!(
                            "neomax {}",
                            a.iter()
                                .map(|a| format!("{a:?}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ))
                        .unwrap_or_else(|e| e.to_string())
                ),
            ),
            Confirm::Quit => popup(
                frame,
                "Stop orchestrator and quit?",
                "This closes the orchestrator opened in this TUI.\nOther sessions are unaffected.\n\ny stops it and quits · any other key cancels",
            ),
        }
    }
}

fn split(area: Rect) -> Vec<Rect> {
    Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(area)
        .to_vec()
}

fn render_table(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    headers: &[&str],
    rows: Vec<Row<'static>>,
    widths: &[Constraint],
    selected: usize,
) {
    let mut state = TableState::default().with_selected(Some(selected));
    frame.render_stateful_widget(
        Table::new(rows, widths)
            .header(Row::new(headers.iter().map(|s| s.to_string())).style(Style::default().fg(DIM)))
            .block(panel(title))
            .row_highlight_style(Style::default().fg(TEAL).add_modifier(Modifier::BOLD))
            .highlight_symbol("› ")
            .column_spacing(1),
        area,
        &mut state,
    );
}

fn status(session: &SessionRecord) -> &'static str {
    if session.working || session.active {
        "recent"
    } else if session.done {
        "idle"
    } else if session.archived {
        "archived"
    } else {
        "quiet"
    }
}

fn tokens(session: &SessionRecord) -> String {
    if session.tokens.total > 0 {
        return count(session.tokens.total);
    }
    if session.tokens.total == 0
        && session.requests == 0
        && session.tokens.input == 0
        && session.tokens.output == 0
    {
        "—".into()
    } else {
        count(
            session.tokens.total.max(
                session
                    .tokens
                    .input
                    .saturating_add(session.tokens.output)
                    .saturating_add(session.tokens.cache_read)
                    .saturating_add(session.tokens.cache_write),
            ),
        )
    }
}

fn count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.)
    } else {
        value.to_string()
    }
}

fn fleet(frame: &mut Frame, app: &App, area: Rect) {
    fleet_view::draw(frame, app, area);
}
fn string(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| {
            value.get(k).filter(|v| !v.is_null()).map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
        })
        .unwrap_or_else(|| "—".into())
}

fn tasks(frame: &mut Frame, app: &App, area: Rect) {
    let parts = split(area);
    let rows = app
        .snapshot
        .tasks
        .iter()
        .map(|t| {
            Row::new(vec![
                clean(&string(t, &["title", "text", "description", "id"])),
                clean(&string(t, &["status", "state"])),
                clean(&string(t, &["project"])),
            ])
        })
        .collect();
    if !app.detail_open {
        render_table(
            frame,
            parts[0],
            "Project tasks",
            &["Task", "State", "Project"],
            rows,
            &[
                Constraint::Min(12),
                Constraint::Length(10),
                Constraint::Length(12),
            ],
            app.selected,
        );
    }
    let text = app.snapshot.tasks.get(app.selected).map(|t| serde_json::to_string_pretty(t).unwrap_or_default()).unwrap_or_else(|| "No durable tasks reported.\n\nTasks created through Neomax appear here. Agent activity is on the Fleet page.".into());
    frame.render_widget(
        Paragraph::new(clean(&text))
            .block(panel("Task details"))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0))
            .style(Style::default().fg(INK)),
        if app.detail_open { area } else { parts[1] },
    );
}

fn quota(window: Option<&Value>, now: i64) -> String {
    let Some(window) = window else {
        return "unknown".into();
    };
    let reset = window.get("resets_at").and_then(|value| {
        value
            .as_f64()
            .filter(|v| v.is_finite())
            .and_then(|v| chrono::DateTime::from_timestamp(v as i64, 0))
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                    .map(|v| v.with_timezone(&chrono::Utc))
            })
    });
    if reset.is_some_and(|r| r.timestamp() <= now) {
        return "reset; awaiting refresh".into();
    }
    let Some(percent) = window
        .get("used_percent")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
    else {
        return "unknown".into();
    };
    let filled = (percent.clamp(0., 100.) / 10.).round() as usize;
    format!(
        "{}{} {:>3.0}%{}",
        "━".repeat(filled),
        "·".repeat(10 - filled),
        percent,
        reset
            .map(|r| format!("  resets {}", r.format("%m-%d %H:%M %:z")))
            .unwrap_or_default()
    )
}

fn usage(frame: &mut Frame, app: &App, area: Rect) {
    let Some(report) = app
        .snapshot
        .usage
        .as_ref()
        .filter(|report| report.days == app.days)
    else {
        frame.render_widget(Paragraph::new("Loading recorded usage…\n\nAccount allowance and plan details are on Accounts.\nd changes the usage window independently of the header.").block(panel("Usage")), area);
        return;
    };
    if app.detail_open {
        let mut text = report.by_model.get(app.selected).map(|row| format!("{}\n\nLast {} days · API estimate ${:.2}\n\nUncached input  {}\nOutput tokens   {}\nCache read      {}\nCache write     {}\nRequests        {}\nErrors          {}\n\nAll discovered local profiles are included, even for work started outside Neomax. Other devices and cloud-only sessions are not synchronized.\n\nCosts use recorded charges or standard model rates. Unrecorded fast-mode or long-context premiums, cache writes and tool fees can be missing. This is not your subscription bill.", row.model, report.days, row.metrics.cost, count(row.metrics.input), count(row.metrics.output), count(row.metrics.cache_read), count(row.metrics.cache_write), row.metrics.requests, row.metrics.errors)).unwrap_or_else(|| "No recorded model usage in this window.".into());
        for warning in &report.warnings {
            text.push_str(&format!("\n\n{warning}"));
        }
        frame.render_widget(
            Paragraph::new(clean(&text))
                .block(panel("Model usage · Esc back"))
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0)),
            area,
        );
        return;
    }
    let sections = Layout::vertical([Constraint::Length(5), Constraint::Min(4)]).split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    "{} DAYS  ·  ${:.2} API-equivalent",
                    report.days, report.grand.cost
                ),
                Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
            )),
            Line::from(format!(
                "{} input  ·  {} output  ·  {} cache read  ·  {} cache write",
                count(report.grand.input),
                count(report.grand.output),
                count(report.grand.cache_read),
                count(report.grand.cache_write)
            )),
            Line::from(format!(
                "{} requests  ·  {} errors  ·  d changes window",
                report.grand.requests, report.grand.errors
            )),
            Line::from(Span::styled(
                if report.warnings.is_empty() {
                    "Local records, not your subscription bill. Enter shows pricing and coverage."
                } else {
                    "Incomplete historical counters. Enter shows the gaps and pricing limits."
                },
                Style::default().fg(DIM),
            )),
        ])
        .wrap(Wrap { trim: false }),
        sections[0],
    );
    let rows = report
        .by_model
        .iter()
        .map(|row| {
            Row::new(vec![
                clean(&row.model),
                count(row.metrics.input),
                count(row.metrics.output),
                count(row.metrics.cache_read),
                format!("${:.2}", row.metrics.cost),
            ])
        })
        .collect();
    render_table(
        frame,
        sections[1],
        "Recorded usage by model",
        &["Model", "Input", "Output", "Cache read", "Estimate"],
        rows,
        &[
            Constraint::Min(15),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
        app.selected,
    );
}
fn launch(frame: &mut Frame, app: &App, area: Rect) {
    let parts = split(area);
    let accounts = app.accounts();
    let models = app.model_choices();
    let values = [
        "Enter to start → Chat".into(),
        app.engine()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Automatic routing".into()),
        if app.launch.more {
            "Hide advanced settings".into()
        } else {
            "Account, model, workers, task…".into()
        },
        app.launch
            .account_name
            .as_ref()
            .map(|n| app.account_label(app.engine().map(|e| e.as_str()).unwrap_or(""), n))
            .or_else(|| accounts.first().cloned())
            .unwrap_or_default(),
        models
            .get(app.launch.model)
            .cloned()
            .unwrap_or_else(|| "Selection unavailable".into()),
        if app.launch.workers == 0 {
            "All eligible providers".into()
        } else {
            neomax_core::Engine::ALL[app.launch.workers - 1].to_string()
        },
        if app.launch.codex_fast {
            "Fast · explicit opt-in".into()
        } else {
            "Standard · default".into()
        },
        if app.launch.prompt.is_empty() {
            "Enter to write initial task".into()
        } else {
            "Enter to edit initial task".into()
        },
    ];
    let labels = [
        "Start Neomax",
        "Provider",
        "More",
        "Account",
        "Model",
        "Workers",
        "Codex speed",
        "Codex speed",
        "Prompt",
    ];
    let rows = labels
        .iter()
        .zip(values)
        .take(if app.launch.more { 8 } else { 3 })
        .map(|(label, value)| Row::new(vec![label.to_string(), clean(&value)]).height(2))
        .collect();
    render_table(
        frame,
        parts[0],
        "Launch Neomax",
        &["", "Enter opens / confirms"],
        rows,
        &[Constraint::Length(13), Constraint::Min(12)],
        app.launch.field,
    );
    let text = format!(
        "READY TO WORK\n\nStart Neomax selects the best eligible account using your routing and quota settings. It opens an interactive orchestrator in Chat, in this directory.\n\nWorkers default to all eligible providers. Codex defaults to standard speed. Use Provider to narrow the choice, or More for advanced settings.\n\nINITIAL TASK{}\n{}\n\n↑↓ selects · Enter opens · ←→ changes pages\nWhile editing a choice: ←→ changes value, Enter saves.\n\nBrowsing makes no model requests.",
        if app.launch.editing {
            " · editing"
        } else {
            ""
        },
        if app.launch.prompt.is_empty() {
            "No initial task. The provider opens interactively."
        } else {
            &app.launch.prompt
        }
    );
    frame.render_widget(
        Paragraph::new(clean(&text))
            .block(panel("Prompt & launch scope"))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0))
            .style(Style::default().fg(INK)),
        parts[1],
    );
}

fn orchestrator(frame: &mut Frame, app: &App, area: Rect) {
    let Some(session) = app.terminal.as_ref() else {
        frame.render_widget(Paragraph::new("Chat with your Neomax orchestrator\n\nPress Enter to open Launch, then Enter to start with automatic routing.\n\nThe provider's interactive terminal appears here. Type messages, respond to prompts and watch its work. Ctrl+] returns to page navigation.\n\nSessions opened elsewhere remain view-only on Fleet.").block(panel("Chat")).wrap(Wrap { trim: false }).style(Style::default().fg(INK)), area);
        return;
    };
    let title = if let Some(exit) = &session.exited {
        format!("Orchestrator · {exit}")
    } else if session.input {
        "Orchestrator · INPUT · Ctrl+] releases focus".into()
    } else {
        "Orchestrator · Enter focuses · PgUp/PgDn history".into()
    };
    let block = panel(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let screen = session.parser.screen();
    for y in 0..inner.height {
        for x in 0..inner.width {
            if let Some(cell) = screen.cell(y, x) {
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut style = Style::default()
                    .fg(terminal_color(cell.fgcolor()))
                    .bg(terminal_color(cell.bgcolor()));
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if cell.inverse() {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                let contents = cell.contents();
                let contents = if contents.is_empty() { " " } else { &contents };
                frame.buffer_mut().set_stringn(
                    inner.x + x,
                    inner.y + y,
                    contents,
                    if cell.is_wide() { 2 } else { 1 },
                    style,
                );
            }
        }
    }
    if session.input && !screen.hide_cursor() {
        let (row, col) = screen.cursor_position();
        if row < inner.height && col < inner.width {
            frame.set_cursor_position((inner.x + col, inner.y + row));
        }
    }
}

fn terminal_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(n) => Color::Indexed(n),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn popup(frame: &mut Frame, title: &str, text: &str) {
    let area = frame.area();
    let width = area.width.saturating_sub(6).min(78);
    let height = area.height.saturating_sub(4).min(19);
    let area = Rect::new(
        (area.width - width) / 2,
        (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(clean(text))
            .block(panel(title).border_style(Style::default().fg(VIOLET)))
            .style(Style::default().fg(INK))
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn populated_fleet_keeps_subagent_tokens_and_pending_runs_visible() {
        let mut app = App::new(BTreeMap::new());
        let mut root = SessionRecord::with_identity("root", neomax_core::Engine::Codex, "1");
        root.label = Some("Review checkout flow".into());
        root.working = true;
        root.model = Some("gpt-6-astra".into());
        root.tokens.total = 48_000;
        app.page = FLEET;
        let mut child = SessionRecord::with_identity("child", neomax_core::Engine::Claude, "2");
        child.label = Some("Test payment retries".into());
        child.parent_id = Some("root".into());
        child.model = Some("claude-fable-5-1".into());
        child.working = true;
        child.tokens.total = 12_300;
        app.sessions = vec![root, child];
        app.snapshot.runs.push(neomax_portal::model::RunView {
            id: "pending".into(),
            engine: "kimi".into(),
            status: "queued".into(),
            prompt: "Inspect worker recovery".into(),
            ..Default::default()
        });
        app.refreshed = Some(std::time::Instant::now());
        app.message.clear();
        let mut terminal = Terminal::new(TestBackend::new(125, 36)).unwrap();
        terminal
            .draw(|frame| draw(frame, &app, Path::new("example/project")))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..36)
            .map(|y| {
                (0..125)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("12.3k"));
        assert!(text.contains("48.0k"));
        assert!(text.contains("queued"));
        assert!(text.contains("Test payment retries"));
        if let Some(directory) = std::env::var_os("NEOMAX_TUI_RENDER_FIXTURES") {
            let cells = (0..36).map(|y| (0..125).map(|x| {
                let cell = &buffer[(x,y)];
                serde_json::json!({"text":cell.symbol(),"fg":format!("{:?}",cell.fg),"bg":format!("{:?}",cell.bg)})
            }).collect::<Vec<_>>()).collect::<Vec<_>>();
            std::fs::write(
                std::path::Path::new(&directory).join("fleet.json"),
                serde_json::to_vec(&cells).unwrap(),
            )
            .unwrap();
        }
    }
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn spend_is_visible_on_every_page_and_accounts_use_email_at_user_terminal_size() {
        let mut app = App::new(BTreeMap::new());
        app.message.clear();
        app.spend = Some(std::sync::Arc::new(neomax_core::usage::UsageReport {
            days: 1,
            now: chrono::Utc::now().timestamp(),
            warnings: vec![],
            grand: neomax_core::usage::UsageMetrics {
                cost: 22.4,
                input: 80_000,
                output: 12_000,
                cache_read: 13_600_000_000,
                cache_write: 229_000_000,
                ..Default::default()
            },
            by_provider: vec![],
            by_account: vec![],
            by_model: vec![],
            by_date: vec![],
            by_session: vec![],
            by_agent: vec![],
            opencode: vec![],
            kimi: vec![],
            grok: vec![],
            pricing: BTreeMap::new(),
        }));
        app.snapshot.engines.insert(
            "codex".into(),
            neomax_portal::model::EngineView {
                accounts: vec![neomax_portal::model::AccountView {
                    n: "1".into(),
                    name: ".codex".into(),
                    email: Some("dev@example.test".into()),
                    authenticated: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        for (page, name) in PAGES.iter().enumerate() {
            app.page = page;
            let mut terminal = Terminal::new(TestBackend::new(91, 37)).unwrap();
            terminal
                .draw(|frame| draw(frame, &app, Path::new("example/project")))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..37)
                .map(|y| (0..91).map(|x| buffer[(x, y)].symbol()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("$22.40"), "missing spend on {name}");
            assert!(
                text.contains("13.6B") && text.contains("229.0M"),
                "missing cache totals on {name}"
            );
            assert!(
                text.contains("1d · all providers"),
                "missing spend scope on {name}"
            );
            assert!(text.contains("24h") && text.contains("7d") && text.contains("30d"));
            if page == LAUNCH {
                assert!(text.contains("Start Neomax"));
                assert!(!text.contains("Codex speed"));
            }
            if page == ACCOUNTS {
                assert!(text.contains("dev@example.test"));
                assert!(text.contains("codex 1"));
            }
            if let Some(directory) = std::env::var_os("NEOMAX_TUI_RENDER_FIXTURES") {
                let cells = (0..37).map(|y| (0..91).map(|x| { let cell = &buffer[(x,y)]; serde_json::json!({"text":cell.symbol(),"fg":format!("{:?}",cell.fg),"bg":format!("{:?}",cell.bg)}) }).collect::<Vec<_>>()).collect::<Vec<_>>();
                std::fs::write(
                    std::path::Path::new(&directory).join(format!("page-{page}.json")),
                    serde_json::to_vec(&cells).unwrap(),
                )
                .unwrap();
            }
        }
        app.spend_range = 2;
        std::sync::Arc::make_mut(app.spend.as_mut().unwrap()).days = 30;
        for (width, height) in [(80, 24), (60, 20)] {
            for (page, name) in PAGES.iter().enumerate() {
                app.page = page;
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| draw(frame, &app, Path::new("project")))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let text = (0..height)
                    .map(|y| {
                        (0..width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for label in [
                    "$22.40",
                    "Cache 13.6B r 229.0M w",
                    "30d · all providers",
                    "Updated",
                    "s range",
                ] {
                    assert!(
                        text.contains(label),
                        "missing {label} at {width}x{height} on {}",
                        name
                    );
                }
                for (y, line) in LOGO.lines().enumerate() {
                    let rendered = (1..17)
                        .map(|x| buffer[(x, y as u16 + 1)].symbol())
                        .collect::<String>();
                    assert!(rendered.starts_with(line), "logo clipped at row {y}");
                }
            }
        }
    }

    #[test]
    fn every_page_renders_at_normal_and_small_sizes_without_a_provider() {
        for (width, height) in [(120, 40), (80, 24), (60, 20), (25, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for page in 0..PAGES.len() {
                let mut app = App::new(BTreeMap::new());
                app.page = page;
                terminal
                    .draw(|frame| draw(frame, &app, Path::new("project")))
                    .unwrap();
            }
        }
    }

    #[test]
    fn missing_or_reset_quota_is_not_rendered_as_available_capacity() {
        assert_eq!(quota(None, 100), "unknown");
        assert_eq!(
            quota(
                Some(&serde_json::json!({"used_percent":99,"resets_at":10})),
                100
            ),
            "reset; awaiting refresh"
        );
        assert!(
            quota(
                Some(&serde_json::json!({"used_percent":99,"resets_at":200})),
                100
            )
            .contains("99%")
        );
    }
}
