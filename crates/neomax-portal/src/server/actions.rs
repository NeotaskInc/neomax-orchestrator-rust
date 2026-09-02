use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::json;

use crate::actions::{ActionIntent, ActionKind, plan_action};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::ActionResponse;
use crate::routes::Route;
use crate::security::log_internal;

use super::PortalServer;

impl<S, E, P> PortalServer<S, E, P>
where
    S: crate::source::PortalSource + 'static,
    E: crate::actions::LocalActionExecutor + 'static,
    P: crate::actions::PrStateResolver + 'static,
{
    pub(super) fn action_response(
        &self,
        target: Route,
        request: &HttpRequest,
    ) -> Result<HttpResponse> {
        let body = match parse_action_body(request) {
            Ok(body) => body,
            Err(error) => {
                log_internal("invalid action body", &error);
                return HttpResponse::json(400, &json!({"error": "invalid action request"}));
            }
        };
        let intent = match intent_from_route(target, body) {
            Ok(intent) => intent,
            Err(error) => {
                log_internal("invalid action intent", &error);
                return HttpResponse::json(400, &json!({"error": "invalid action request"}));
            }
        };
        let plan = match plan_action(&self.source.action_context(), &intent) {
            Ok(plan) => plan,
            Err(error) => {
                log_internal("invalid local action", &error);
                return HttpResponse::json(400, &json!({"error": "invalid local action"}));
            }
        };
        if plan.confirmation_required && !intent.confirmed() {
            return HttpResponse::json(
                409,
                &json!({
                    "error": "explicit confirmation is required for this action",
                    "confirmation_required": true,
                    "plan": plan.view(),
                }),
            );
        }
        let plan_view = plan.view();
        let execution = self.executor.execute(&plan)?;
        let response = ActionResponse {
            accepted: true,
            executed: execution.executed,
            operation: plan.operation,
            engine: plan.engine,
            account: plan.account,
            run_id: plan.run_id,
            confirmation_required: plan.confirmation_required,
            message: execution.message,
            pid: execution.pid,
            plan: Some(plan_view),
        };
        self.json(response)
    }

    pub(super) fn require_same_origin(&self, request: &HttpRequest) -> Result<()> {
        let origin = request
            .headers
            .get("origin")
            .ok_or_else(|| anyhow::anyhow!("Origin header is required for local actions"))?;
        let allowed = [
            format!("http://127.0.0.1:{}", self.bind.port()),
            format!("http://localhost:{}", self.bind.port()),
            format!("http://[::1]:{}", self.bind.port()),
        ];
        if allowed.iter().any(|value| value == origin) {
            Ok(())
        } else {
            bail!("cross-origin local action rejected")
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionBody {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    confirm: bool,
}

fn parse_action_body(request: &HttpRequest) -> Result<ActionBody> {
    if request.body.is_empty() {
        return Ok(ActionBody::default());
    }
    serde_json::from_slice(&request.body)
        .map_err(|error| anyhow::anyhow!("invalid action JSON: {error}"))
}

fn intent_from_route(target: Route, body: ActionBody) -> Result<ActionIntent> {
    match target {
        Route::Connect { engine, account } => Ok(ActionIntent::Connect {
            engine,
            account,
            confirm: body.confirm,
        }),
        Route::Pause {
            engine,
            account,
            paused,
        } => Ok(ActionIntent::Pause {
            engine,
            account,
            paused,
            confirm: body.confirm,
        }),
        Route::RunAction { action, id } => Ok(ActionIntent::Run {
            action: action.parse::<ActionKind>()?,
            run_id: id,
            confirm: body.confirm,
        }),
        Route::Action => {
            let action = body
                .action
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("action is required"))?;
            match action {
                "connect" => Ok(ActionIntent::Connect {
                    engine: body
                        .engine
                        .ok_or_else(|| anyhow::anyhow!("engine is required"))?,
                    account: body
                        .account
                        .ok_or_else(|| anyhow::anyhow!("account is required"))?,
                    confirm: body.confirm,
                }),
                "pause" | "unpause" => Ok(ActionIntent::Pause {
                    engine: body
                        .engine
                        .ok_or_else(|| anyhow::anyhow!("engine is required"))?,
                    account: body
                        .account
                        .ok_or_else(|| anyhow::anyhow!("account is required"))?,
                    paused: action == "pause",
                    confirm: body.confirm,
                }),
                value => Ok(ActionIntent::Run {
                    action: value.parse::<ActionKind>()?,
                    run_id: body
                        .run_id
                        .ok_or_else(|| anyhow::anyhow!("run_id is required"))?,
                    confirm: body.confirm,
                }),
            }
        }
        _ => bail!("not a local action route"),
    }
}
