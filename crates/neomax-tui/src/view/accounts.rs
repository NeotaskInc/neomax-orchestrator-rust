use super::*;

fn state(account: &neomax_portal::model::AccountView, now: i64) -> &'static str {
    if account.paused {
        "Paused"
    } else if account.credential.as_ref().is_some_and(|credential| {
        credential.health == neomax_core::providers::catalog::CredentialHealth::RefreshAvailable
    }) {
        "Refreshable"
    } else if account.credential.as_ref().is_some_and(|credential| {
        credential.health == neomax_core::providers::catalog::CredentialHealth::LoginRequired
    }) {
        "Login needed"
    } else if account.token_expired {
        "Expired"
    } else if account.cooldown_until > now {
        "Cooling"
    } else if account.authenticated {
        "Connected"
    } else {
        "Login needed"
    }
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).split(area);
    let total = app
        .snapshot
        .engines
        .values()
        .map(|view| view.accounts.len())
        .sum::<usize>();
    let tabs = std::iter::once(("All".to_string(), total))
        .chain(neomax_core::Engine::ALL.iter().map(|engine| {
            (
                engine.to_string(),
                app.snapshot
                    .engines
                    .get(engine.as_str())
                    .map(|view| view.accounts.len())
                    .unwrap_or(0),
            )
        }))
        .enumerate()
        .map(|(index, (label, count))| {
            Span::styled(
                format!(" {label} {count} "),
                if index == app.account_provider {
                    Style::default().fg(Color::Black).bg(TEAL)
                } else {
                    Style::default().fg(DIM)
                },
            )
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(Line::from(tabs)), layout[0]);
    let parts = split(layout[1]);
    let accounts = app.profile_rows();
    if !app.detail_open {
        let rows = accounts
            .iter()
            .map(|(engine, account)| {
                Row::new(vec![
                    Line::from(Span::styled(
                        engine.to_string(),
                        Style::default().fg(VIOLET),
                    )),
                    Line::from(app.account_label(engine, &account.n)),
                    Line::from(Span::styled(
                        state(account, app.snapshot.now),
                        Style::default().fg(if account.eligibility.orchestrator_eligible {
                            TEAL
                        } else {
                            AMBER
                        }),
                    )),
                ])
            })
            .collect();
        render_table(
            frame,
            parts[0],
            "Accounts · [ ] provider",
            &["Provider", "Email / profile", "State"],
            rows,
            &[
                Constraint::Length(8),
                Constraint::Min(12),
                Constraint::Length(12),
            ],
            app.selected,
        );
    }
    let mut lines = vec![];
    if let Some((engine, account)) = accounts.get(app.selected) {
        lines.push(Line::from(Span::styled(
            app.account_label(engine, &account.n),
            Style::default().fg(TEAL).add_modifier(Modifier::BOLD),
        )));
        for (key, value) in [
            ("Provider", engine.to_string()),
            ("Profile", format!("{} · {}", account.n, account.name)),
            ("State", state(account, app.snapshot.now).into()),
            (
                "Credential evidence",
                account
                    .credential
                    .as_ref()
                    .map(|credential| {
                        format!("{:?} (local; remote access unverified)", credential.health)
                    })
                    .unwrap_or_else(|| "Unknown".into()),
            ),
            (
                "Plan",
                account
                    .plan
                    .clone()
                    .unwrap_or_else(|| "Not reported".into()),
            ),
            (
                "Authentication",
                account
                    .auth_method
                    .clone()
                    .unwrap_or_else(|| "Not reported".into()),
            ),
            (
                "Orchestrator",
                if account.eligibility.orchestrator_eligible {
                    "Eligible"
                } else {
                    "Not eligible"
                }
                .into(),
            ),
            (
                "Workers",
                if account.worker_eligible {
                    "Eligible"
                } else {
                    "Not eligible"
                }
                .into(),
            ),
            ("Role", account.role.clone()),
            ("Profile directory", account.dir.display().to_string()),
        ] {
            lines.push(Line::from(vec![
                Span::styled(format!("{key}: "), Style::default().fg(DIM)),
                Span::raw(clean(&value)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "ALLOWANCE USED",
            Style::default().fg(VIOLET),
        )));
        let usage = account.usage.as_ref();
        for (label, key) in [("5 hours", "five_hour"), ("Shared 7 days", "seven_day")] {
            lines.push(Line::from(format!(
                "{label}: {}",
                quota(usage.and_then(|u| u.get(key)), app.snapshot.now)
            )));
        }
        if usage.is_some_and(|u| {
            u.get("stale").and_then(Value::as_bool) == Some(true)
                || u.get("expired").and_then(Value::as_bool) == Some(true)
        }) {
            lines.push(Line::from(Span::styled(
                "Cached allowance is stale; refresh required.",
                Style::default().fg(AMBER),
            )));
        }
        if let Some(windows) = usage
            .and_then(|u| u.get("model_weekly"))
            .and_then(Value::as_object)
        {
            for (name, window) in windows {
                lines.push(Line::from(format!(
                    "{name} 7 days: {}",
                    quota(Some(window), app.snapshot.now)
                )));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from("l → Launch with this account"));
        lines.push(Line::from("Enter expands details · [ ] filters provider"));
        lines.push(Line::from("Browsing does not switch accounts or log in."));
    } else {
        lines.push(Line::from("No profiles found for this provider."));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel("Account details"))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        if app.detail_open { layout[1] } else { parts[1] },
    );
}
