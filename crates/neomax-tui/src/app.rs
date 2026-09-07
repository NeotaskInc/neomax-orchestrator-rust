use std::collections::BTreeMap;
use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use neomax_core::Engine;
use neomax_core::providers::catalog::MapEnvironment;
use neomax_core::sessions::SessionRecord;
use neomax_portal::{model::PortalSnapshot, source::FilesystemPortalSource};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    Options,
    data::{Feed, clean},
    terminal::{Session, key_bytes},
    view,
};

pub const LAUNCH: usize = 0;
pub const CHAT: usize = 1;
pub const FLEET: usize = 2;
pub const TASKS: usize = 3;
pub const ACCOUNTS: usize = 4;
pub const USAGE: usize = 5;
pub const PAGES: [&str; 6] = ["Launch", "Chat", "Fleet", "Tasks", "Accounts", "Usage"];

#[derive(Default)]
pub struct Launch {
    pub provider: usize,
    pub account: usize,
    pub account_name: Option<String>,
    pub model: usize,
    pub workers: usize,
    pub codex_fast: bool,
    pub prompt: String,
    pub field: usize,
    pub editing: bool,
    pub choosing: bool,
    pub more: bool,
}

pub struct App {
    pub page: usize,
    pub selected: usize,
    pub detail_scroll: u16,
    pub days: u32,
    pub fleet_days: u32,
    pub fleet_view: usize,
    pub account_provider: usize,
    pub detail_open: bool,
    pub spend_range: usize,
    pub spend: Option<Arc<neomax_core::usage::UsageReport>>,
    pub usage_error: Option<String>,
    pub snapshot: PortalSnapshot,
    pub sessions: Vec<SessionRecord>,
    pub log: Option<(String, String)>,
    pub launch: Launch,
    pub models: BTreeMap<Engine, Vec<String>>,
    pub terminal: Option<Session>,
    pub help: bool,
    pub confirmation: Option<Confirm>,
    pub message: String,
    pub refreshed: Option<Instant>,
    pub refreshing: bool,
    pub filter: String,
    pub searching: bool,
}

#[derive(Clone, Copy)]
pub enum Confirm {
    Launch,
    Quit,
}

impl App {
    pub fn new(models: BTreeMap<Engine, Vec<String>>) -> Self {
        Self {
            page: LAUNCH,
            selected: 0,
            detail_scroll: 0,
            days: 7,
            fleet_days: 7,
            fleet_view: 0,
            account_provider: 0,
            detail_open: false,
            spend_range: 0,
            spend: None,
            usage_error: None,
            snapshot: PortalSnapshot::default(),
            sessions: Vec::new(),
            log: None,
            launch: Launch::default(),
            models,
            terminal: None,
            help: false,
            confirmation: None,
            message: "Reading local activity · ←→ pages · q quits".into(),
            refreshed: None,
            refreshing: true,
            filter: String::new(),
            searching: false,
        }
    }

    pub fn engine(&self) -> Option<Engine> {
        self.launch
            .provider
            .checked_sub(1)
            .map(|i| Engine::ALL[i % Engine::ALL.len()])
    }

    pub fn accounts(&self) -> Vec<String> {
        let mut accounts = vec!["Automatic".into()];
        if let Some(engine) = self.engine() {
            if let Some(view) = self.snapshot.engines.get(engine.as_str()) {
                accounts.extend(
                    view.accounts
                        .iter()
                        .filter(|a| a.eligibility.orchestrator_eligible)
                        .map(|a| a.n.clone()),
                );
            }
        }
        accounts
    }

    pub fn model_choices(&self) -> Vec<String> {
        let mut choices = vec!["Configured default".into()];
        if let Some(engine) = self.engine() {
            if let Some(models) = self.models.get(&engine) {
                choices.extend(models.iter().cloned());
            }
        }
        choices
    }

    pub fn launch_args(&self) -> Result<Vec<String>> {
        let mut args = vec!["orchestrator".into()];
        if let Some(engine) = self.engine() {
            args.extend(["--engine".into(), engine.as_str().into()]);
        }
        if self.launch.account > 0 {
            let accounts = self.accounts();
            let account = self
                .launch
                .account_name
                .as_ref()
                .filter(|name| accounts.contains(name))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Selected account is no longer available; choose an account again"
                    )
                })?;
            args.extend(["--account".into(), account.clone()]);
        }
        if self.launch.model > 0 {
            let models = self.model_choices();
            let model = models
                .get(self.launch.model)
                .ok_or_else(|| anyhow::anyhow!("Selected model is no longer available"))?;
            args.extend(["--model".into(), model.clone()]);
        }
        let workers = if self.launch.workers == 0 {
            "all"
        } else {
            Engine::ALL[(self.launch.workers - 1) % Engine::ALL.len()].as_str()
        };
        args.extend(["--workers".into(), workers.into()]);
        args.push(
            if self.launch.codex_fast {
                "--codex-fast"
            } else {
                "--codex-standard"
            }
            .into(),
        );
        if !self.launch.prompt.trim().is_empty() {
            args.extend(["--".into(), self.launch.prompt.clone()]);
        }
        Ok(args)
    }

    pub fn filtered_sessions(&self) -> Vec<&SessionRecord> {
        let query = self.filter.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| {
                (self.fleet_view != 1 || s.active || s.working)
                    && (self.fleet_view != 2 || self.owned(s))
                    && (query.is_empty()
                        || format!(
                            "{} {} {} {} {} {}",
                            s.id,
                            s.engine,
                            s.account,
                            s.label.as_deref().unwrap_or_default(),
                            s.model.as_deref().unwrap_or_default(),
                            s.activity
                                .as_ref()
                                .and_then(|a| a.task.as_ref())
                                .map(|t| t.text.as_str())
                                .unwrap_or_default()
                        )
                        .to_lowercase()
                        .contains(&query))
            })
            .collect()
    }

    pub fn selected_session(&self) -> Option<&SessionRecord> {
        self.filtered_sessions().get(self.selected).copied()
    }

    pub fn selected_run(&self) -> Option<String> {
        if self.page != FLEET {
            return None;
        }
        let Some(session) = self.selected_session() else {
            return self
                .unmatched_runs()
                .get(self.selected.saturating_sub(self.filtered_sessions().len()))
                .map(|r| r.id.clone());
        };
        self.snapshot
            .runs
            .iter()
            .find(|r| {
                r.engine == session.engine.as_str() && r.session.as_deref() == Some(&session.id)
            })
            .map(|r| r.id.clone())
    }

    pub fn unmatched_runs(&self) -> Vec<&neomax_portal::model::RunView> {
        self.snapshot
            .runs
            .iter()
            .filter(|run| {
                (self.fleet_view != 1 || run.status == "running")
                    && !self.sessions.iter().any(|s| {
                        run.engine == s.engine.as_str() && run.session.as_deref() == Some(&s.id)
                    })
                    && (self.filter.is_empty()
                        || format!("{} {} {}", run.id, run.engine, run.prompt)
                            .to_lowercase()
                            .contains(&self.filter.to_lowercase()))
            })
            .collect()
    }

    pub fn spend_days(&self) -> u32 {
        [1, 7, 30][self.spend_range % 3]
    }

    pub fn profile_rows(&self) -> Vec<(&str, &neomax_portal::model::AccountView)> {
        self.snapshot
            .engines
            .iter()
            .filter(|(engine, _)| {
                self.account_provider == 0
                    || Engine::ALL[self.account_provider - 1].as_str() == engine.as_str()
            })
            .flat_map(|(engine, view)| {
                view.accounts
                    .iter()
                    .map(move |account| (engine.as_str(), account))
            })
            .collect()
    }

    pub fn account_label(&self, engine: &str, number: &str) -> String {
        self.snapshot
            .engines
            .get(engine)
            .and_then(|view| {
                view.accounts
                    .iter()
                    .find(|account| account.n == number || account.name == number)
            })
            .map(|account| {
                account
                    .email
                    .clone()
                    .or_else(|| account.display_name.clone())
                    .unwrap_or_else(|| format!("Account {} · email not reported", account.n))
            })
            .unwrap_or_else(|| format!("Account {number} · email not reported"))
    }

    pub fn request(&self, feed: &Feed) {
        feed.request_view(
            self.days,
            self.fleet_days,
            self.spend_days(),
            self.selected_run(),
        );
    }

    pub fn owned(&self, session: &SessionRecord) -> bool {
        let mut current = session;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            if !seen.insert(current.id.as_str()) {
                return false;
            }
            if current.worker
                || current.orchestrator
                || self.snapshot.runs.iter().any(|run| {
                    run.engine == current.engine.as_str()
                        && (run.session.as_deref() == Some(&current.id)
                            || run.orch_session.as_deref() == Some(&current.id))
                })
            {
                return true;
            }
            let Some(parent) = current.parent_id.as_deref().and_then(|id| {
                self.sessions.iter().find(|candidate| {
                    candidate.engine == current.engine
                        && candidate.account == current.account
                        && candidate.id == id
                })
            }) else {
                return false;
            };
            current = parent;
        }
    }

    fn cycle_field(&mut self, direction: isize) {
        match self.launch.field {
            1 => {
                self.launch.provider = cycle(self.launch.provider, 6, direction);
                self.launch.account = 0;
                self.launch.account_name = None;
                self.launch.model = 0;
            }
            3 => {
                let accounts = self.accounts();
                let index = self
                    .launch
                    .account_name
                    .as_ref()
                    .and_then(|name| accounts.iter().position(|a| a == name))
                    .unwrap_or(0);
                self.launch.account = cycle(index, accounts.len(), direction);
                self.launch.account_name =
                    (self.launch.account > 0).then(|| accounts[self.launch.account].clone());
            }
            4 => {
                self.launch.model = cycle(self.launch.model, self.model_choices().len(), direction)
            }
            5 => self.launch.workers = cycle(self.launch.workers, 6, direction),
            6 => self.launch.codex_fast = !self.launch.codex_fast,
            _ => {}
        }
    }

    fn navigate(&mut self, page: usize) {
        self.page = page % PAGES.len();
        self.selected = 0;
        self.detail_scroll = 0;
        self.detail_open = false;
        self.launch.choosing = false;
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.input = false;
        }
    }
}

fn cycle(value: usize, count: usize, direction: isize) -> usize {
    (value as isize + direction).rem_euclid(count.max(1) as isize) as usize
}

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

pub fn run(options: Options) -> Result<()> {
    let source = FilesystemPortalSource::new(&options.home, &options.state)
        .with_discovery_environment(
            MapEnvironment::new(std::env::vars())
                .with_home(options.home.clone())
                .with_current_dir(options.cwd.clone()),
        )
        .with_provider_runtime(&options.runtime);
    let models = options
        .runtime
        .catalog_arc()
        .providers
        .iter()
        .map(|(engine, provider)| (*engine, provider.models.clone()))
        .collect();
    let feed = Feed::start(Arc::new(source));
    let mut app = App::new(models);
    enable_raw_mode()?;
    let _restore = Restore;
    execute!(io::stdout(), EnterAlternateScreen, EnableBracketedPaste)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let result = event_loop(&mut terminal, &mut app, &feed, &options);
    drop(app);
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    feed: &Feed,
    options: &Options,
) -> Result<()> {
    loop {
        while let Ok(update) = feed.updates.try_recv() {
            let identity = (app.page == FLEET)
                .then(|| {
                    app.selected_session()
                        .map(|s| (s.engine, s.account.clone(), s.id.clone()))
                })
                .flatten();
            if let Some(mut snapshot) = update.snapshot {
                if snapshot.usage.is_none() {
                    snapshot.usage = app.snapshot.usage.take();
                }
                app.snapshot = snapshot;
                app.refreshed = Some(Instant::now());
            }
            if let Some(sessions) = update.sessions {
                app.sessions = sessions;
                if let Some(identity) = identity {
                    app.selected = app
                        .filtered_sessions()
                        .iter()
                        .position(|s| (s.engine, s.account.clone(), s.id.clone()) == identity)
                        .unwrap_or(0);
                }
            }
            if update.log.is_some() {
                app.log = update.log;
            }
            if let Some(spend) = update.spend {
                if spend.days == app.spend_days() {
                    app.spend = Some(spend);
                }
            }
            app.usage_error = update.usage_error;
            app.refreshing = update.progress.is_some();
            app.message = update
                .error
                .map(|e| format!("Refresh warning: {}", clean(&e)))
                .or(update.progress)
                .unwrap_or_default();
            if app.message.is_empty() && !app.snapshot.errors.is_empty() {
                app.message = format!(
                    "{}: {}",
                    app.snapshot.errors[0].component,
                    clean(&app.snapshot.errors[0].message)
                );
            }
            if let Some(run) = app.selected_run() {
                if app.log.as_ref().is_none_or(|(id, _)| id != &run) {
                    app.request(feed);
                }
            }
        }
        if let Some(session) = app.terminal.as_mut() {
            if let Err(error) = session.poll() {
                app.message = format!("Terminal: {error}");
            }
            let area = terminal.size()?;
            let inner =
                view::terminal_area(ratatui::layout::Rect::new(0, 0, area.width, area.height));
            session.resize(inner.height, inner.width)?;
        }
        terminal.draw(|frame| view::draw(frame, app, &options.cwd))?;
        if !event::poll(Duration::from_millis(80))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if handle_key(
                    app,
                    key,
                    feed,
                    options,
                    terminal.size()?.width,
                    terminal.size()?.height,
                )? {
                    break;
                }
            }
            Event::Paste(value) => {
                if app.page == CHAT && app.terminal.as_ref().is_some_and(|t| t.input) {
                    if let Some(session) = app.terminal.as_mut() {
                        if let Err(e) = session.paste(&value) {
                            app.message = e.to_string();
                        }
                    }
                } else if app.page == LAUNCH && app.launch.editing {
                    app.launch.prompt.push_str(&clean(&value));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn handle_key(
    app: &mut App,
    key: KeyEvent,
    feed: &Feed,
    options: &Options,
    width: u16,
    height: u16,
) -> Result<bool> {
    if app.page == CHAT && app.terminal.as_ref().is_some_and(|t| t.input) {
        let session = app.terminal.as_mut().expect("terminal was checked");
        if matches!(key.code, KeyCode::Char(']' | '5'))
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            session.input = false;
        } else {
            let bytes = key_bytes(key, session.parser.screen().application_cursor());
            if let Err(e) = session.send(&bytes) {
                app.message = e.to_string();
            }
        }
        return Ok(false);
    }
    if let Some(confirm) = app.confirmation {
        app.confirmation = None;
        if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'))
            || (matches!(confirm, Confirm::Launch) && key.code == KeyCode::Enter)
        {
            match confirm {
                Confirm::Quit => return Ok(true),
                Confirm::Launch => match app.launch_args().and_then(|args| {
                    let inner =
                        view::terminal_area(ratatui::layout::Rect::new(0, 0, width, height));
                    Session::spawn(
                        &options.executable,
                        &options.cwd,
                        &args,
                        inner.height,
                        inner.width,
                    )
                }) {
                    Ok(session) => {
                        app.terminal = Some(session);
                        app.page = CHAT;
                    }
                    Err(error) => app.message = format!("Launch failed: {error}"),
                },
            }
        }
        return Ok(false);
    }
    if app.help {
        app.help = false;
        return Ok(false);
    }
    if app.searching {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.searching = false,
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(c) => app.filter.push(c),
            _ => {}
        }
        app.selected = 0;
        return Ok(false);
    }
    if app.page == LAUNCH && app.launch.editing {
        match key.code {
            KeyCode::Esc => app.launch.editing = false,
            KeyCode::Enter => app.launch.prompt.push('\n'),
            KeyCode::Backspace => {
                app.launch.prompt.pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                app.launch.prompt.push(c)
            }
            _ => {}
        }
        return Ok(false);
    }
    if app.page == LAUNCH && app.launch.choosing {
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => app.cycle_field(-1),
            KeyCode::Right | KeyCode::Char('l') => app.cycle_field(1),
            KeyCode::Enter | KeyCode::Esc => app.launch.choosing = false,
            _ => {}
        }
        return Ok(false);
    }
    if app.page == ACCOUNTS
        && (matches!(key.code, KeyCode::Char('[' | ']'))
            || (matches!(key.code, KeyCode::Left | KeyCode::Right)
                && key.modifiers.contains(KeyModifiers::SHIFT)))
    {
        let direction = if matches!(key.code, KeyCode::Left | KeyCode::Char('[')) {
            -1
        } else {
            1
        };
        app.account_provider = cycle(app.account_provider, Engine::ALL.len() + 1, direction);
        app.selected = 0;
        app.detail_scroll = 0;
        return Ok(false);
    }
    if app.detail_open
        && matches!(
            key.code,
            KeyCode::Esc | KeyCode::Enter | KeyCode::Up | KeyCode::Down
        )
    {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.detail_open = false,
            KeyCode::Up => app.detail_scroll = app.detail_scroll.saturating_sub(1),
            KeyCode::Down => app.detail_scroll = app.detail_scroll.saturating_add(1),
            _ => {}
        }
        return Ok(false);
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('c')
            if key.code == KeyCode::Char('q') || key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            if app.terminal.as_ref().is_some_and(|t| t.exited.is_none()) {
                app.confirmation = Some(Confirm::Quit);
            } else {
                return Ok(true);
            }
        }
        KeyCode::Char('?') => app.help = true,
        KeyCode::Char(c @ '1'..='6') => app.navigate(c as usize - '1' as usize),
        KeyCode::Tab => app.navigate(app.page + 1),
        KeyCode::BackTab => app.navigate((app.page + PAGES.len() - 1) % PAGES.len()),
        KeyCode::Left => app.navigate((app.page + PAGES.len() - 1) % PAGES.len()),
        KeyCode::Right => app.navigate(app.page + 1),
        KeyCode::Char('c') => app.navigate(CHAT),
        KeyCode::Char('l') if app.page == ACCOUNTS => {
            if let Some((engine, account)) = app
                .profile_rows()
                .get(app.selected)
                .map(|(e, a)| (e.to_string(), (*a).clone()))
            {
                if account.eligibility.orchestrator_eligible {
                    app.launch.provider = Engine::ALL
                        .iter()
                        .position(|e| e.as_str() == engine)
                        .unwrap_or(0)
                        + 1;
                    app.launch.account_name = Some(account.n.clone());
                    app.launch.account = app
                        .accounts()
                        .iter()
                        .position(|n| n == &account.n)
                        .unwrap_or(0);
                    app.launch.model = 0;
                    app.launch.field = 0;
                    app.navigate(LAUNCH);
                } else {
                    app.message = "This profile is not eligible for an orchestrator. Check its account details.".into();
                }
            }
        }
        KeyCode::Char('s') => {
            app.spend_range = (app.spend_range + 1) % 3;
            app.request(feed);
        }
        KeyCode::Char('v') if app.page == FLEET => {
            app.fleet_view = (app.fleet_view + 1) % 3;
            app.selected = 0;
            app.detail_scroll = 0;
            app.request(feed);
        }
        KeyCode::Char('/') if app.page == FLEET => app.searching = true,
        KeyCode::Char('r') => {
            app.request(feed);
            app.refreshing = true;
        }
        KeyCode::Char('d') if app.page == FLEET || app.page == USAGE => {
            let days = if app.page == FLEET {
                &mut app.fleet_days
            } else {
                &mut app.days
            };
            *days = match *days {
                1 => 7,
                7 => 30,
                _ => 1,
            };
            app.request(feed);
            app.refreshing = true;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.page == LAUNCH {
                app.launch.field = cycle(app.launch.field, if app.launch.more { 8 } else { 3 }, -1);
            } else {
                app.selected = app.selected.saturating_sub(1);
                app.detail_scroll = 0;
                app.request(feed);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.page == LAUNCH {
                app.launch.field = cycle(app.launch.field, if app.launch.more { 8 } else { 3 }, 1);
            } else {
                let count = match app.page {
                    FLEET => app.filtered_sessions().len() + app.unmatched_runs().len(),
                    TASKS => app.snapshot.tasks.len(),
                    ACCOUNTS => app.profile_rows().len(),
                    USAGE => app
                        .snapshot
                        .usage
                        .as_ref()
                        .map(|report| report.by_model.len())
                        .unwrap_or(0),
                    _ => 0,
                };
                app.selected = (app.selected + 1).min(count.saturating_sub(1));
                app.detail_scroll = 0;
                app.request(feed);
            }
        }
        KeyCode::Enter if app.page == LAUNCH => match app.launch.field {
            7 => app.launch.editing = true,
            2 => app.launch.more = !app.launch.more,
            0 => {
                if app.terminal.as_ref().is_some_and(|t| t.exited.is_none()) {
                    app.message =
                        "An orchestrator is already open. Press c to return to Chat.".into();
                } else {
                    app.confirmation = Some(Confirm::Launch);
                }
            }
            _ => app.launch.choosing = true,
        },
        KeyCode::Enter | KeyCode::Char('i') if app.page == CHAT => {
            if let Some(session) = app.terminal.as_mut() {
                session.input = session.exited.is_none();
                session.parser.set_scrollback(0);
                session.scroll = 0;
            } else {
                app.navigate(LAUNCH);
            }
        }
        KeyCode::Enter if matches!(app.page, FLEET | TASKS | ACCOUNTS | USAGE) => {
            app.detail_open = true;
            app.detail_scroll = 0;
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            let up = key.code == KeyCode::PageUp;
            if app.page == CHAT {
                if let Some(session) = app.terminal.as_mut() {
                    session.scroll = if up {
                        (session.scroll + 10).min(2000)
                    } else {
                        session.scroll.saturating_sub(10)
                    };
                    session.parser.set_scrollback(session.scroll);
                }
            } else {
                app.detail_scroll = if up {
                    app.detail_scroll.saturating_sub(10)
                } else {
                    app.detail_scroll.saturating_add(10)
                };
            }
        }
        KeyCode::Esc => {
            app.filter.clear();
            app.selected = 0;
        }
        _ => {}
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_children_inherit_only_matching_account_and_provider_ownership() {
        let mut app = App::new(BTreeMap::new());
        let mut root = SessionRecord::with_identity("root", Engine::Codex, "1");
        root.orchestrator = true;
        let mut child = SessionRecord::with_identity("child", Engine::Codex, "1");
        child.parent_id = Some("root".into());
        app.sessions = vec![root, child.clone()];
        assert!(app.owned(&child));
        child.account = "2".into();
        assert!(!app.owned(&child));
        child.account = "1".into();
        child.engine = Engine::Claude;
        assert!(!app.owned(&child));
        assert_eq!(app.page, LAUNCH);
        assert_eq!(app.launch.field, 0);
        assert!(!app.launch.more);
        assert_eq!(PAGES[CHAT], "Chat");
    }

    #[test]
    fn codex_fast_is_off_by_default_and_enabled_only_by_a_choice() {
        let mut app = App::new(BTreeMap::new());
        assert!(
            app.launch_args()
                .unwrap()
                .contains(&"--codex-standard".into())
        );
        app.launch.field = 6;
        app.cycle_field(1);
        assert!(app.launch_args().unwrap().contains(&"--codex-fast".into()));
    }

    #[test]
    fn removed_account_selection_is_rejected_instead_of_selecting_a_replacement() {
        let mut app = App::new(BTreeMap::new());
        app.launch.provider = 1;
        app.launch.account = 1;
        app.launch.account_name = Some("removed".into());
        assert!(app.launch_args().is_err());
    }

    #[test]
    fn provider_change_resets_account_and_model_without_implicitly_selecting_opus() {
        let mut app = App::new(BTreeMap::from([(
            Engine::Claude,
            vec!["claude-opus-5".into()],
        )]));
        app.launch.account = 2;
        app.launch.model = 3;
        app.launch.field = 1;
        app.cycle_field(1);
        assert_eq!(app.launch.account, 0);
        assert_eq!(app.launch.model, 0);
        assert!(
            !app.launch_args()
                .unwrap()
                .iter()
                .any(|v| v.contains("opus"))
        );
    }

    #[test]
    fn prompt_is_one_literal_argument_with_independent_worker_scope() {
        let mut app = App::new(BTreeMap::new());
        app.launch.provider = 1;
        app.launch.workers = 2;
        app.launch.prompt = "$(no-shell)\n--model evil".into();
        assert_eq!(
            app.launch_args().unwrap(),
            [
                "orchestrator",
                "--engine",
                "claude",
                "--workers",
                "codex",
                "--codex-standard",
                "--",
                "$(no-shell)\n--model evil"
            ]
        );
    }
}
