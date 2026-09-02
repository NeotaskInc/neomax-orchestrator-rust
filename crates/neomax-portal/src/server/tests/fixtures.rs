use std::collections::BTreeMap;

use crate::actions::{
    ActionContext, ActionExecution, ActionPlan, LocalActionExecutor, PrStateResolver,
};
use crate::http::HttpRequest;
use crate::model::{ModesResponse, PortalSnapshot, PrStateView, RunDiff};
use crate::source::{FilesystemPortalSource, PortalSource};
use neomax_core::runs::HistorySummary;
use neomax_core::sessions::SessionRecord;
use neomax_core::usage::{PriceCatalog, UsageReport};

pub(super) struct EmptySource;

impl PortalSource for EmptySource {
    fn action_context(&self) -> ActionContext {
        let root = std::env::temp_dir().join(format!(
            "neomax-portal-server-fixture-{}",
            std::process::id()
        ));
        let home = root.join("home");
        let state = root.join("state");
        let mut context = ActionContext::from_home(home, state);
        context.neomax_binary = std::env::current_exe()
            .expect("test executable")
            .to_string_lossy()
            .into_owned();
        context
    }

    fn status(&self, now: i64, _days: u32) -> anyhow::Result<PortalSnapshot> {
        Ok(PortalSnapshot {
            now,
            ..PortalSnapshot::default()
        })
    }

    fn history(&self, _limit: usize) -> anyhow::Result<Vec<HistorySummary>> {
        Ok(Vec::new())
    }

    fn modes(&self) -> anyhow::Result<ModesResponse> {
        Ok(ModesResponse::default())
    }

    fn usage(&self, days: u32, now: i64) -> anyhow::Result<UsageReport> {
        Ok(neomax_core::usage::build_usage_report(
            &[],
            days,
            now,
            &PriceCatalog::default(),
        ))
    }

    fn sessions(&self, _days: u32, _now: i64) -> anyhow::Result<Vec<SessionRecord>> {
        Ok(Vec::new())
    }

    fn run_diff(&self, id: &str) -> anyhow::Result<RunDiff> {
        Ok(RunDiff {
            id: id.into(),
            ..RunDiff::default()
        })
    }

    fn run_log(&self, _id: &str, _limit: usize) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

#[derive(Default)]
pub(super) struct RecordingExecutor {
    plans: std::sync::Mutex<Vec<ActionPlan>>,
}

impl LocalActionExecutor for RecordingExecutor {
    fn execute(&self, plan: &ActionPlan) -> anyhow::Result<ActionExecution> {
        self.plans.lock().unwrap().push(plan.clone());
        Ok(ActionExecution {
            executed: true,
            pid: Some(42),
            message: "recorded".into(),
        })
    }
}

pub(super) struct StubPrState;

impl PrStateResolver for StubPrState {
    fn resolve(&self, url: &str) -> anyhow::Result<PrStateView> {
        Ok(PrStateView {
            url: url.into(),
            available: true,
            state: Some("OPEN".into()),
            ..PrStateView::default()
        })
    }
}

pub(super) struct FailingPrState;

impl PrStateResolver for FailingPrState {
    fn resolve(&self, _url: &str) -> anyhow::Result<PrStateView> {
        anyhow::bail!("provider command failed at /fixture/private token=secret")
    }
}

pub(super) fn request(method: &str, path: &str, body: &[u8], origin: Option<&str>) -> HttpRequest {
    let mut headers = BTreeMap::new();
    headers.insert("host".into(), "localhost:8787".into());
    if method == "POST" {
        headers.insert("content-type".into(), "application/json".into());
    }
    if let Some(origin) = origin {
        headers.insert("origin".into(), origin.into());
    }
    HttpRequest {
        method: method.into(),
        target: path.into(),
        path: path.split('?').next().unwrap().into(),
        query: path
            .split_once('?')
            .map(|(_, value)| {
                value
                    .split('&')
                    .filter_map(|part| part.split_once('='))
                    .map(|(key, value)| (key.into(), value.into()))
                    .collect()
            })
            .unwrap_or_default(),
        headers,
        body: body.to_vec(),
    }
}

pub(super) fn filesystem_source(temp: &tempfile::TempDir) -> FilesystemPortalSource {
    FilesystemPortalSource::new(temp.path(), temp.path().join("state"))
}
