use uuid::Uuid;

use crate::providers::catalog;
use crate::runs::{RunRecord, RunStatus};
use crate::Engine;

use super::model::{ModelResolver, NoModelOverrides};
use super::types::FailoverTarget;

pub const SAME_PROVIDER_NOTE: &str = "\n\nNOTE: a previous attempt at this task may have left partial work in the current directory. Run `git status` if this is a repository and inspect existing files first. Continue the task to completion instead of starting over.";
pub const CROSS_PROVIDER_NOTE: &str = "\n\nNOTE: a previous attempt by a different provider ran in this same working directory and hit a usage limit. Its committed work is on the current branch and partial work may remain in the tree. Run `git status` and `git log --oneline -5`, inspect what exists, and continue the task to completion. Do not discard prior work.";

pub fn apply_failover(run: &mut RunRecord, target: &FailoverTarget) {
    apply_failover_with_resolver(run, target, &NoModelOverrides);
}

pub fn apply_failover_with_resolver(
    run: &mut RunRecord,
    target: &FailoverTarget,
    models: &dyn ModelResolver,
) {
    run.remember_session();
    if !run.tried.contains(&run.profile) {
        run.tried.push(run.profile.clone());
    }
    let source_engine = run.engine;
    run.engine = target.account.engine;
    run.profile = target.account.profile.clone();
    run.attempt = run.attempt.saturating_add(1);
    run.status = RunStatus::Running;
    run.ended = None;
    run.worker_pid = None;
    run.result_text = None;
    run.usage = None;
    run.resets_at = None;
    run.limit_window = None;
    run.error_detail = None;

    if target.crosses_provider || source_engine != run.engine {
        apply_provider_defaults(run, models);
        run.prompt_to_send = Some(format!("{}{}", run.prompt, CROSS_PROVIDER_NOTE));
    } else {
        run.prompt_to_send = Some(format!("{}{}", run.prompt, SAME_PROVIDER_NOTE));
    }
    run.session = (run.engine == Engine::Claude).then(|| Uuid::new_v4().to_string());
}

fn apply_provider_defaults(run: &mut RunRecord, models: &dyn ModelResolver) {
    run.model = models
        .model_for(run.engine)
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| catalog::default_model_id(run.engine).into());
    match run.engine {
        Engine::Claude => {
            run.ultra = false;
        }
        Engine::Codex => {
            run.effort = Some(match run.effort.as_deref() {
                Some("max") => "xhigh".into(),
                Some(value) => value.into(),
                None => "high".into(),
            });
            run.ultra = false;
        }
        Engine::Opencode | Engine::Kimi | Engine::Grok => {
            run.effort = None;
            run.ultra = false;
        }
    }
}
