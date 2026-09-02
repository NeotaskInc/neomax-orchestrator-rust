use anyhow::{Result, bail};
use neomax_core::runs::{RunStatus, RunStore};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::launch;
use crate::output;
use crate::parser;

pub(crate) fn rerun(context: &RuntimeContext, args: &[String], resume: bool) -> Result<()> {
    let id = run_id(args)?;
    let run = launch::rerun(context, &id, resume)?;
    if parser::has(args, "--json") {
        return output::json(&json!({
            "id": run.id,
            "status": run.status.as_str(),
            "engine": run.engine,
            "attempt": run.attempt,
            "session": run.session,
            "log": run.log,
        }));
    }
    println!("{} {} attempt {}", run.id, run.status.as_str(), run.attempt);
    Ok(())
}

pub(crate) fn kill(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let store = RunStore::new(&context.paths.runs);
    let ids = if parser::has(args, "--all") {
        store
            .all()?
            .into_iter()
            .filter(|run| run.status == RunStatus::Running)
            .map(|run| run.id)
            .collect::<Vec<_>>()
    } else {
        let id = run_id(args)?;
        vec![id]
    };
    if ids.is_empty() {
        println!("no running runs");
        return Ok(());
    }
    let mut killed = Vec::new();
    for id in ids {
        let run = store.load(&id)?;
        if run.status != RunStatus::Running {
            continue;
        }
        if let Some(pid) = run.worker_pid {
            crate::process::terminate_worker(pid)?;
        }
        let updated = store.update(&id, |run| {
            run.killed = true;
            run.status = RunStatus::Aborted;
            run.ended = Some(context.now);
            Ok(())
        })?;
        killed.push(updated.id);
    }
    if parser::has(args, "--json") {
        return output::json(&json!({"killed": killed}));
    }
    for id in killed {
        println!("killed {id}");
    }
    Ok(())
}

fn run_id(args: &[String]) -> Result<String> {
    let ids = args
        .iter()
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .collect::<Vec<_>>();
    match ids.as_slice() {
        [id] if !id.trim().is_empty() => Ok(id.clone()),
        [] => bail!("a run id is required"),
        _ => bail!("exactly one run id is required"),
    }
}

#[cfg(test)]
mod tests {
    use super::run_id;

    #[test]
    fn accepts_one_run_id_and_ignores_output_flags() {
        assert_eq!(run_id(&["run-1".into(), "--json".into()]).unwrap(), "run-1");
        assert!(run_id(&[]).is_err());
        assert!(run_id(&["one".into(), "two".into()]).is_err());
    }
}
