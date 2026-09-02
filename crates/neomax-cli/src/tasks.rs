use anyhow::{Result, bail};
use neomax_core::tasks::{TaskPatch, TaskStatus, TaskStore};

use crate::context::RuntimeContext;
use crate::error;
use crate::output;
use crate::parser;

pub fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("list");
    let rest = args.get(1..).unwrap_or(&[]);
    let store = TaskStore::new(&context.paths.tasks);
    match subcommand {
        "add" => add(context, &store, rest),
        "done" | "start" | "doing" | "block" | "blocked" | "drop" | "dropped" | "reopen"
        | "todo" | "merge" | "merged" => update_status(context, &store, subcommand, rest),
        "status" => set_status(context, &store, rest),
        "note" => note(context, &store, rest),
        "link" => link(context, &store, rest),
        "rm" => remove(&store, rest),
        "list" => list(context, &store, rest),
        other if other.starts_with('-') => list(context, &store, args),
        other => Err(error::usage_error(anyhow::anyhow!(
            "unknown task subcommand {other}"
        ))),
    }
}

fn add(context: &RuntimeContext, store: &TaskStore, args: &[String]) -> Result<()> {
    let project =
        error::usage(parser::value(args, "--project"))?.or_else(|| context.project_for_cwd());
    let note = error::usage(parser::value(args, "--note"))?;
    let status = error::usage(parser::value(args, "--status"))?
        .map(|value| error::usage(parse_status(&value)))
        .transpose()?
        .unwrap_or_default();
    let title = error::usage(parser::positional(
        args,
        &["--project", "--note", "--status"],
    ))?
    .join(" ")
    .trim()
    .to_owned();
    let task = store.add(&title, project, status, note, context.now)?;
    println!(
        "task {} added{}: {}",
        task.id,
        task.project
            .as_deref()
            .map(|value| format!(" [{value}]"))
            .unwrap_or_default(),
        task.title
    );
    Ok(())
}

fn update_status(
    context: &RuntimeContext,
    store: &TaskStore,
    command: &str,
    args: &[String],
) -> Result<()> {
    let status = match command {
        "done" => TaskStatus::Done,
        "start" | "doing" => TaskStatus::Doing,
        "block" | "blocked" => TaskStatus::Blocked,
        "drop" | "dropped" => TaskStatus::Dropped,
        "reopen" | "todo" => TaskStatus::Todo,
        "merge" | "merged" => TaskStatus::Merged,
        _ => unreachable!(),
    };
    let ids = error::usage(parser::positional(args, &[]))?;
    if ids.is_empty() {
        return Err(error::usage_error(anyhow::anyhow!(
            "task {command}: at least one task id is required"
        )));
    }
    for id in ids {
        if store
            .update(
                &id,
                TaskPatch {
                    status: Some(status.clone()),
                    ..TaskPatch::default()
                },
                context.now,
            )?
            .is_some()
        {
            println!("task {id} -> {}", status_name(&status));
        } else {
            bail!("no task {id}");
        }
    }
    Ok(())
}

fn set_status(context: &RuntimeContext, store: &TaskStore, args: &[String]) -> Result<()> {
    let Some(id) = args.first() else {
        return Err(error::usage_error(anyhow::anyhow!(
            "task status: task id and status are required"
        )));
    };
    let Some(raw) = args.get(1) else {
        return Err(error::usage_error(anyhow::anyhow!(
            "task status: task id and status are required"
        )));
    };
    let status = error::usage(parse_status(raw))?;
    if store
        .update(
            id,
            TaskPatch {
                status: Some(status.clone()),
                ..TaskPatch::default()
            },
            context.now,
        )?
        .is_none()
    {
        bail!("no task {id}");
    }
    println!("task {id} -> {}", status_name(&status));
    Ok(())
}

fn note(context: &RuntimeContext, store: &TaskStore, args: &[String]) -> Result<()> {
    let Some(id) = args.first() else {
        return Err(error::usage_error(anyhow::anyhow!(
            "task note: task id and note are required"
        )));
    };
    let note = args[1..].join(" ");
    if note.trim().is_empty() {
        return Err(error::usage_error(anyhow::anyhow!(
            "task note: task id and note are required"
        )));
    }
    if store
        .update(
            id,
            TaskPatch {
                note: Some(note),
                ..TaskPatch::default()
            },
            context.now,
        )?
        .is_none()
    {
        bail!("no task {id}");
    }
    println!("task {id}: noted");
    Ok(())
}

fn link(context: &RuntimeContext, store: &TaskStore, args: &[String]) -> Result<()> {
    let Some(id) = args.first() else {
        return Err(error::usage_error(anyhow::anyhow!(
            "task link: task id and run id are required"
        )));
    };
    let Some(run_id) = args.get(1) else {
        return Err(error::usage_error(anyhow::anyhow!(
            "task link: task id and run id are required"
        )));
    };
    if store
        .update(
            id,
            TaskPatch {
                run_id: Some(run_id.clone()),
                ..TaskPatch::default()
            },
            context.now,
        )?
        .is_none()
    {
        bail!("no task {id}");
    }
    println!("task {id} linked to run {run_id}");
    Ok(())
}

fn remove(store: &TaskStore, args: &[String]) -> Result<()> {
    let ids = error::usage(parser::positional(args, &[]))?;
    if ids.is_empty() {
        return Err(error::usage_error(anyhow::anyhow!(
            "task rm: at least one task id is required"
        )));
    }
    for id in ids {
        if store.remove(&id)?.is_some() {
            println!("task {id} removed");
        } else {
            println!("no task {id}");
        }
    }
    Ok(())
}

fn list(context: &RuntimeContext, store: &TaskStore, args: &[String]) -> Result<()> {
    let all = parser::has(args, "--all");
    let all_projects = parser::has(args, "--all-projects");
    let project = if all_projects {
        None
    } else {
        error::usage(parser::value(args, "--project"))?.or_else(|| context.project_for_cwd())
    };
    let mut tasks = store.list(project.as_deref(), all);
    tasks.sort_by_key(|task| (status_order(&task.status), std::cmp::Reverse(task.updated)));
    if parser::has(args, "--json") {
        return output::json(&tasks);
    }
    if tasks.is_empty() {
        let scope = if all_projects {
            "any project".to_owned()
        } else if let Some(project) = project {
            format!("project '{project}'")
        } else {
            "this location".to_owned()
        };
        println!(
            "no {} tasks for {scope} -> add one: neomax task add \"<title>\"",
            if all { "" } else { "open" }
        );
        return Ok(());
    }
    print!("TASKS{}", if all { " (all)" } else { " (open)" });
    if all_projects {
        print!(" · all projects");
    } else if let Some(project) = project {
        print!(" · {project}");
    }
    println!();
    for task in tasks {
        let project = if all_projects {
            task.project
                .as_deref()
                .map(|value| format!(" [{value}]"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let runs = if task.runs.is_empty() {
            String::new()
        } else {
            format!(
                " <-{}",
                task.runs
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        println!(
            "  {:<3} {:<8} {}{}{}",
            task.id,
            status_name(&task.status).to_ascii_uppercase(),
            task.title,
            project,
            runs
        );
    }
    Ok(())
}

fn parse_status(value: &str) -> Result<TaskStatus> {
    match value.to_ascii_lowercase().as_str() {
        "todo" => Ok(TaskStatus::Todo),
        "doing" => Ok(TaskStatus::Doing),
        "blocked" => Ok(TaskStatus::Blocked),
        "done" => Ok(TaskStatus::Done),
        "merged" => Ok(TaskStatus::Merged),
        "dropped" => Ok(TaskStatus::Dropped),
        _ => bail!("task status must be todo, doing, blocked, done, merged, or dropped"),
    }
}

fn status_name(status: &TaskStatus) -> &str {
    status.as_str()
}

fn status_order(status: &TaskStatus) -> u8 {
    match status {
        TaskStatus::Doing => 0,
        TaskStatus::Blocked => 1,
        TaskStatus::Todo => 2,
        TaskStatus::Done => 3,
        TaskStatus::Merged => 4,
        TaskStatus::Dropped => 5,
        TaskStatus::Unknown(_) => 6,
    }
}
