use anyhow::Result;
use serde_json::json;

use crate::actions::validate_pr_url;
use crate::http::{HttpRequest, HttpResponse};
use crate::routes::{Route, route};
use crate::security::{log_internal, require_json_content_type, require_loopback_host};

use super::PortalServer;

const INDEX: &[u8] = include_bytes!("../../assets/index.html");
const STYLES: &[u8] = include_bytes!("../../assets/styles.css");
const APP: &[u8] = include_bytes!("../../assets/app.js");
const API: &[u8] = include_bytes!("../../assets/api.js");
const RENDER: &[u8] = include_bytes!("../../assets/render.js");
const FORMAT: &[u8] = include_bytes!("../../assets/format.js");
const DELIVERY: &[u8] = include_bytes!("../../assets/delivery.js");

impl<S, E, P> PortalServer<S, E, P>
where
    S: crate::source::PortalSource + 'static,
    E: crate::actions::LocalActionExecutor + 'static,
    P: crate::actions::PrStateResolver + 'static,
{
    pub(super) fn response(&self, request: &HttpRequest) -> Result<HttpResponse> {
        if let Err(error) = require_loopback_host(request, self.bind) {
            log_internal("rejected Host header", &error);
            return HttpResponse::json(403, &json!({"error": "invalid local Host header"}));
        }
        if request.method != "GET" && request.method != "POST" {
            return HttpResponse::json(
                405,
                &json!({
                    "error": "portal accepts GET and guarded local POST actions",
                    "allow": "GET, POST"
                }),
            );
        }
        let target = match route(request, self.default_days) {
            Ok(target) => target,
            Err(error) => {
                log_internal("invalid route", &error);
                return HttpResponse::json(400, &json!({"error": "invalid portal request"}));
            }
        };
        if request.method == "POST" {
            if !target_is_action(&target) {
                return HttpResponse::json(
                    405,
                    &json!({
                        "error": "this portal endpoint is read-only",
                        "allow": "GET"
                    }),
                );
            }
            if let Err(error) = require_json_content_type(request) {
                log_internal("rejected action Content-Type", &error);
                return HttpResponse::json(
                    415,
                    &json!({"error": "action requests require Content-Type: application/json"}),
                );
            }
            if let Err(error) = self.require_same_origin(request) {
                return HttpResponse::json(403, &json!({"error": error.to_string()}));
            }
            return self.action_response(target, request);
        }
        match target {
            Route::Index => Ok(HttpResponse::text(
                200,
                "text/html; charset=utf-8",
                INDEX.to_vec(),
            )),
            Route::Asset { name } => Ok(HttpResponse::text(
                200,
                asset_type(name),
                asset_body(name).to_vec(),
            )),
            Route::Status => self.json(self.source.status(now(), self.default_days)?),
            Route::History { limit } => self.json(self.source.history(limit)?),
            Route::Modes => self.json(self.source.modes()?),
            Route::Usage { days } => self.json(self.source.usage(days, now())?),
            Route::Sessions { days } => self.json(self.source.sessions(days, now())?),
            Route::Subagents { days } => {
                let rows = self
                    .source
                    .sessions(days, now())?
                    .into_iter()
                    .filter(|row| row.is_child())
                    .collect::<Vec<_>>();
                self.json(rows)
            }
            Route::RunDiff { id } => self.json(self.source.run_diff(&id)?),
            Route::Log { id } => Ok(HttpResponse::text(
                200,
                "text/plain; charset=utf-8",
                self.source.run_log(&id, 400_000)?.into_bytes(),
            )),
            Route::Projects => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.projects)
            }
            Route::Plans => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.plans)
            }
            Route::Issues => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.issues)
            }
            Route::Worktrees => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.worktrees)
            }
            Route::Tasks => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.tasks)
            }
            Route::Queue => {
                let snapshot = self.source.status(now(), self.default_days)?;
                self.json(snapshot.queue)
            }
            Route::PrState => {
                let Some(url) = request.query.get("url") else {
                    return HttpResponse::json(400, &json!({"error": "url is required"}));
                };
                if let Err(error) = validate_pr_url(url) {
                    log_internal("invalid PR URL", &error);
                    return HttpResponse::json(400, &json!({"error": "invalid pull request URL"}));
                }
                self.json(self.pr_state.resolve(url)?)
            }
            Route::Connect { .. }
            | Route::Pause { .. }
            | Route::RunAction { .. }
            | Route::Action
            | Route::NotFound => HttpResponse::json(404, &json!({"error":"not found"})),
        }
    }

    pub(super) fn json<T: serde::Serialize>(&self, value: T) -> Result<HttpResponse> {
        HttpResponse::json(200, &value)
    }
}

pub(super) fn target_is_action(target: &Route) -> bool {
    matches!(
        target,
        Route::Connect { .. } | Route::Pause { .. } | Route::RunAction { .. } | Route::Action
    )
}

fn asset_body(name: &str) -> &'static [u8] {
    match name {
        "styles.css" => STYLES,
        "app.js" => APP,
        "api.js" => API,
        "render.js" => RENDER,
        "format.js" => FORMAT,
        "delivery.js" => DELIVERY,
        _ => b"",
    }
}

fn asset_type(name: &str) -> &'static str {
    if name.ends_with(".css") {
        "text/css; charset=utf-8"
    } else {
        "text/javascript; charset=utf-8"
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
