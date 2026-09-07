use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use neomax_core::sessions::{SessionRecord, flatten_native_children};
use neomax_core::usage::UsageReport;
use neomax_portal::model::PortalSnapshot;
use neomax_portal::source::PortalSource;

#[derive(Clone, Default)]
pub struct Selection {
    pub days: u32,
    pub fleet_days: u32,
    pub spend_days: u32,
    pub run: Option<String>,
}

pub struct Refresh {
    pub snapshot: Option<PortalSnapshot>,
    pub sessions: Option<Vec<SessionRecord>>,
    pub log: Option<(String, String)>,
    pub error: Option<String>,
    pub progress: Option<String>,
    pub spend: Option<Arc<UsageReport>>,
    pub usage_error: Option<String>,
}

pub struct Feed {
    requests: mpsc::SyncSender<()>,
    history_requests: mpsc::SyncSender<()>,
    usage_requests: mpsc::SyncSender<()>,
    selection: Arc<Mutex<Selection>>,
    stopped: Arc<AtomicBool>,
    pub updates: mpsc::Receiver<Refresh>,
}

#[derive(Clone, Default)]
struct History {
    records: Arc<Vec<SessionRecord>>,
    progress: Option<String>,
    error: Option<String>,
    complete: bool,
}

impl Feed {
    pub fn start(source: Arc<dyn PortalSource>) -> Self {
        let (requests, input) = mpsc::sync_channel::<()>(1);
        let selection = Arc::new(Mutex::new(Selection {
            days: 7,
            fleet_days: 7,
            spend_days: 1,
            run: None,
        }));
        let current = Arc::clone(&selection);
        let stopped = Arc::new(AtomicBool::new(false));
        let history = Arc::new(Mutex::new(History {
            progress: Some("Scanning session history · Tab pages".into()),
            ..History::default()
        }));
        let (history_requests, history_input) = mpsc::sync_channel(1);
        let history_source = Arc::clone(&source);
        let history_selection = Arc::clone(&selection);
        let shared_history = Arc::clone(&history);
        let history_stopped = Arc::clone(&stopped);
        let wake_status = requests.clone();
        thread::spawn(move || {
            loop {
                if history_stopped.load(Ordering::Relaxed) {
                    break;
                }
                let days = match history_selection.lock() {
                    Ok(value) => value.fleet_days,
                    Err(_) => break,
                };
                let first = shared_history
                    .lock()
                    .map(|value| !value.complete)
                    .unwrap_or(false);
                let result = history_source.sessions_with_progress(
                    days,
                    chrono::Utc::now().timestamp(),
                    &mut |records, label| {
                        if history_stopped.load(Ordering::Relaxed) {
                            return false;
                        }
                        if let Ok(mut state) = shared_history.lock() {
                            if first {
                                state.records = Arc::new(records.to_vec());
                            }
                            state.progress = Some(format!(
                                "{label} · {} sessions found · ←→ pages",
                                records.len()
                            ));
                        }
                        let _ = wake_status.try_send(());
                        true
                    },
                );
                if let Ok(mut state) = shared_history.lock() {
                    match result {
                        Ok(records) => {
                            state.records = Arc::new(records);
                            state.complete = true;
                            state.error = None;
                        }
                        Err(error) => state.error = Some(error.to_string()),
                    }
                    state.progress = None;
                }
                let _ = wake_status.try_send(());
                if let Err(mpsc::RecvTimeoutError::Disconnected) =
                    history_input.recv_timeout(Duration::from_secs(5))
                {
                    break;
                }
            }
        });
        let reports = Arc::new(Mutex::new(BTreeMap::<u32, Arc<UsageReport>>::new()));
        let usage_error = Arc::new(Mutex::new(BTreeMap::<u32, String>::new()));
        let (usage_requests, usage_input) = mpsc::sync_channel(1);
        let usage_source = Arc::clone(&source);
        let usage_selection = Arc::clone(&selection);
        let usage_reports = Arc::clone(&reports);
        let usage_errors = Arc::clone(&usage_error);
        let usage_stopped = Arc::clone(&stopped);
        let wake_status = requests.clone();
        thread::spawn(move || {
            loop {
                if usage_stopped.load(Ordering::Relaxed) {
                    break;
                }
                let selected = match usage_selection.lock() {
                    Ok(value) => value.clone(),
                    Err(_) => break,
                };
                for days in [selected.spend_days, selected.days] {
                    if usage_stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let now = chrono::Utc::now().timestamp();
                    let fresh = usage_reports
                        .lock()
                        .ok()
                        .and_then(|reports| {
                            reports
                                .get(&days)
                                .map(|report| now.saturating_sub(report.now) < 15)
                        })
                        .unwrap_or(false);
                    if fresh {
                        continue;
                    }
                    match usage_source.usage(days, now) {
                        Ok(report) => {
                            if let Ok(mut reports) = usage_reports.lock() {
                                reports.insert(days, Arc::new(report));
                            }
                            if let Ok(mut error) = usage_errors.lock() {
                                error.remove(&days);
                            }
                        }
                        Err(error) => {
                            if let Ok(mut value) = usage_errors.lock() {
                                value.insert(days, error.to_string());
                            }
                        }
                    }
                    let _ = wake_status.try_send(());
                }
                if let Err(mpsc::RecvTimeoutError::Disconnected) =
                    usage_input.recv_timeout(Duration::from_secs(15))
                {
                    break;
                }
            }
        });
        let (output, updates) = mpsc::sync_channel(1);
        let status_stopped = Arc::clone(&stopped);
        thread::spawn(move || {
            let mut last_usage = None;
            loop {
                if status_stopped.load(Ordering::Relaxed) {
                    break;
                }
                let selected = match current.lock() {
                    Ok(value) => value.clone(),
                    Err(_) => break,
                };
                let now = chrono::Utc::now().timestamp();
                let history = match history.lock() {
                    Ok(value) => value.clone(),
                    Err(_) => break,
                };
                let snapshot =
                    source.status_with_sessions(now, selected.fleet_days, &history.records, false);
                let log = selected.run.as_ref().map(|id| {
                    (
                        id.clone(),
                        source
                            .run_log(id, 160)
                            .unwrap_or_else(|e| format!("Output unavailable: {e}")),
                    )
                });
                let mut errors = Vec::new();
                let mut snapshot = snapshot.map_err(|e| errors.push(e.to_string())).ok();
                let mut spend = None;
                if let Ok(reports) = reports.lock() {
                    spend = reports.get(&selected.spend_days).cloned();
                    if let (Some(snapshot), Some(report)) =
                        (snapshot.as_mut(), reports.get(&selected.days))
                    {
                        let version = (report.days, report.now);
                        if last_usage != Some(version) {
                            snapshot.usage = Some(report.as_ref().clone());
                            last_usage = Some(version);
                        }
                    }
                }
                if let Some(error) = history.error {
                    errors.push(error);
                }
                if output
                    .send(Refresh {
                        snapshot,
                        sessions: Some(unique_sessions(history.records.as_ref().clone())),
                        log,
                        error: (!errors.is_empty()).then(|| errors.join("; ")),
                        progress: history.progress,
                        spend,
                        usage_error: usage_error
                            .lock()
                            .ok()
                            .and_then(|error| error.get(&selected.spend_days).cloned()),
                    })
                    .is_err()
                {
                    break;
                }
                match input.recv_timeout(Duration::from_secs(1)) {
                    Ok(()) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Self {
            requests,
            history_requests,
            usage_requests,
            selection,
            stopped,
            updates,
        }
    }

    #[cfg(test)]
    pub fn request(&self, days: u32, run: Option<String>) {
        self.request_view(days, days, 1, run);
    }

    pub fn request_view(&self, days: u32, fleet_days: u32, spend_days: u32, run: Option<String>) {
        if let Ok(mut selection) = self.selection.lock() {
            if selection.fleet_days != fleet_days {
                let _ = self.history_requests.try_send(());
            }
            if selection.days != days || selection.spend_days != spend_days {
                let _ = self.usage_requests.try_send(());
            }
            *selection = Selection {
                days,
                fleet_days,
                spend_days,
                run,
            };
        }
        let _ = self.requests.try_send(());
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

pub fn unique_sessions(records: Vec<SessionRecord>) -> Vec<SessionRecord> {
    let mut seen = BTreeSet::new();
    let mut records = flatten_native_children(records);
    records.retain(|s| seen.insert((s.engine, s.account.clone(), s.id.clone())));
    records.sort_by_key(|s| (!s.working, !s.active, std::cmp::Reverse(s.last_active)));
    records
}

pub fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::Engine;

    struct SlowSource {
        local: neomax_portal::source::FilesystemPortalSource,
        release: Mutex<mpsc::Receiver<()>>,
        block_usage: bool,
        usage_entered: Option<mpsc::Sender<()>>,
    }

    impl PortalSource for SlowSource {
        fn status(&self, now: i64, days: u32) -> anyhow::Result<PortalSnapshot> {
            self.local.status(now, days)
        }
        fn status_with_sessions(
            &self,
            now: i64,
            days: u32,
            sessions: &[SessionRecord],
            include_usage: bool,
        ) -> anyhow::Result<PortalSnapshot> {
            self.local
                .status_with_sessions(now, days, sessions, include_usage)
        }
        fn sessions(&self, _: u32, _: i64) -> anyhow::Result<Vec<SessionRecord>> {
            if !self.block_usage {
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))?;
            }
            Ok(vec![SessionRecord::with_identity(
                "history-loaded",
                Engine::Codex,
                "1",
            )])
        }
        fn history(&self, limit: usize) -> anyhow::Result<Vec<neomax_core::runs::HistorySummary>> {
            self.local.history(limit)
        }
        fn modes(&self) -> anyhow::Result<neomax_portal::model::ModesResponse> {
            self.local.modes()
        }
        fn usage(&self, days: u32, now: i64) -> anyhow::Result<neomax_core::usage::UsageReport> {
            if self.block_usage && days == 1 {
                if let Some(entered) = &self.usage_entered {
                    let _ = entered.send(());
                }
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))?;
            }
            self.local.usage(days, now)
        }
        fn run_diff(&self, id: &str) -> anyhow::Result<neomax_portal::model::RunDiff> {
            self.local.run_diff(id)
        }
        fn run_log(&self, id: &str, _: usize) -> anyhow::Result<String> {
            Ok(format!("live output for {id}"))
        }
    }

    #[test]
    fn dashboard_and_selected_output_refresh_while_history_is_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let (release, blocked) = mpsc::channel();
        let feed = Feed::start(Arc::new(SlowSource {
            local: neomax_portal::source::FilesystemPortalSource::new(
                temp.path(),
                temp.path().join("state"),
            ),
            release: Mutex::new(blocked),
            block_usage: false,
            usage_entered: None,
        }));
        let initial = feed.updates.recv_timeout(Duration::from_secs(1));
        feed.request(7, Some("worker-one".into()));
        let mut selected_output = false;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Ok(update) = feed.updates.recv_timeout(Duration::from_millis(100)) {
                if update.log.as_ref().is_some_and(|(id, output)| {
                    id == "worker-one" && output == "live output for worker-one"
                }) {
                    selected_output = true;
                    break;
                }
            }
        }
        release.send(()).unwrap();
        let mut loaded = false;
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if let Ok(update) = feed.updates.recv_timeout(Duration::from_millis(100)) {
                if update.sessions.as_ref().is_some_and(|records| {
                    records.iter().any(|record| record.id == "history-loaded")
                }) {
                    loaded = true;
                    break;
                }
            }
        }
        assert!(initial.unwrap().snapshot.is_some());
        assert!(selected_output, "history held up the selected run output");
        assert!(loaded, "completed history was not published");
    }

    #[test]
    fn selected_output_remains_responsive_during_usage_and_ranges_stay_independent() {
        let temp = tempfile::tempdir().unwrap();
        let (release, blocked) = mpsc::channel();
        let (usage_entered, entered) = mpsc::channel();
        let feed = Feed::start(Arc::new(SlowSource {
            local: neomax_portal::source::FilesystemPortalSource::new(
                temp.path(),
                temp.path().join("state"),
            ),
            release: Mutex::new(blocked),
            block_usage: true,
            usage_entered: Some(usage_entered),
        }));
        entered.recv_timeout(Duration::from_secs(1)).unwrap();
        feed.request_view(30, 7, 1, Some("live-run".into()));
        let started = Instant::now();
        loop {
            let update = feed.updates.recv_timeout(Duration::from_secs(1)).unwrap();
            if update.log.as_ref().is_some_and(|(id, _)| id == "live-run") {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(1));
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        println!(
            "TUI_BLOCKED_USAGE responsive_ms={}",
            started.elapsed().as_millis()
        );
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let (mut spend, mut usage) = (false, false);
        while Instant::now() < deadline && !(spend && usage) {
            let update = feed.updates.recv_timeout(Duration::from_secs(1)).unwrap();
            spend |= update.spend.is_some_and(|report| report.days == 1);
            usage |= update
                .snapshot
                .and_then(|snapshot| snapshot.usage)
                .is_some_and(|report| report.days == 30);
        }
        assert!(
            spend && usage,
            "spend and usage windows were not both published"
        );
        assert_eq!(feed.selection.lock().unwrap().fleet_days, 7);
    }

    #[test]
    fn identity_includes_provider_and_account_and_keeps_orphan_children() {
        let first = SessionRecord::with_identity("same", Engine::Claude, "1");
        let second = SessionRecord::with_identity("same", Engine::Codex, "1");
        let mut child = SessionRecord::with_identity("child", Engine::Claude, "2");
        child.parent_id = Some("missing".into());
        assert_eq!(
            unique_sessions(vec![first.clone(), first, second, child]).len(),
            3
        );
    }

    #[test]
    fn state_cannot_emit_terminal_control_sequences() {
        assert!(!clean("\u{1b}]52;clipboard\u{7}\nhello").contains('\u{1b}'));
    }
}
