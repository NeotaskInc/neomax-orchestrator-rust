use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct ExecutionReport {
    pub(super) invocation: String,
    pub(super) run_id: String,
    pub(super) status: String,
    pub(super) engine: String,
    pub(super) account: String,
    pub(super) model: String,
    pub(super) session: Option<String>,
    pub(super) log: Option<String>,
    pub(super) worker_scope: String,
}
