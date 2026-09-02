use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::error;
use crate::models;
use crate::output;

pub fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let json_output = args.iter().any(|arg| arg == "--json");
    let Some(action) = args
        .iter()
        .find(|arg| !arg.starts_with('-'))
        .map(String::as_str)
    else {
        return show(context, json_output);
    };
    let command_args = without_json(args, action);
    match action {
        "show" => show(context, json_output),
        "models" => show_models(context, json_output),
        "set" => set(context, &command_args[1..]),
        "set-model" => set_model_args(context, &command_args[1..]),
        "unset-model" => unset_model_args(context, &command_args[1..]),
        "unset" if command_args.len() == 3 && command_args[1] == "model" => {
            unset_model_args(context, &[command_args[2].clone()])
        }
        "unset" => unset_model_args(context, &command_args[1..]),
        _ => usage(),
    }
}

fn without_json(args: &[String], action: &str) -> Vec<String> {
    let mut values = vec![action.to_owned()];
    values.extend(
        args.iter()
            .filter(|arg| arg.as_str() != action && arg.as_str() != "--json")
            .cloned(),
    );
    values
}

fn set(context: &RuntimeContext, args: &[String]) -> Result<()> {
    match args {
        [key, value] if key == "max-subagents" => set_max_subagents(context, value),
        [key, value] if key == "max-sessions-per-account" => {
            set_max_sessions_per_account(context, value)
        }
        [key, engine, model] if key == "model" => {
            set_model_args(context, &[engine.clone(), model.clone()])
        }
        [key, model] if key.ends_with("-model") => set_model_args(
            context,
            &[key.trim_end_matches("-model").into(), model.clone()],
        ),
        _ => usage(),
    }
}

fn set_model_args(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let [engine, model] = args else {
        return usage();
    };
    let engine = error::usage(models::parse_engine(engine))?;
    let model = error::usage(models::validate_model(model.clone()))?;
    let path = context.model_config_path();
    let mut overrides = context.model_overrides()?;
    overrides.set(engine, model.clone());
    overrides.save(&path)?;
    println!("model[{engine}] = {model}");
    println!("saved = {}", path.display());
    Ok(())
}

fn unset_model_args(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let [engine] = args else {
        return usage();
    };
    let engine = error::usage(models::parse_engine(engine.trim_end_matches("-model")))?;
    let path = context.model_config_path();
    let mut overrides = context.model_overrides()?;
    overrides.clear(engine);
    overrides.save(&path)?;
    println!("model[{engine}] = default");
    println!("saved = {}", path.display());
    Ok(())
}

fn set_max_subagents(context: &RuntimeContext, value: &str) -> Result<()> {
    let parsed = error::usage(
        value
            .parse::<u32>()
            .with_context(|| "max-subagents must be a positive integer"),
    )?;
    if parsed == 0 {
        bail!("max-subagents must be a positive integer");
    }
    let mut file = neomax_core::SettingsFile::load(&context.settings.config_path)?;
    file.concurrency.max_subagents = parsed;
    file.save(&context.settings.config_path)?;
    println!("max_subagents = {parsed}");
    println!("saved = {}", context.settings.config_path.display());
    Ok(())
}

fn set_max_sessions_per_account(context: &RuntimeContext, value: &str) -> Result<()> {
    let parsed = error::usage(
        value
            .parse::<u32>()
            .with_context(|| "max-sessions-per-account must be a positive integer"),
    )?;
    if parsed == 0 {
        bail!("max-sessions-per-account must be a positive integer");
    }
    let mut file = neomax_core::SettingsFile::load(&context.settings.config_path)?;
    file.concurrency.max_sessions_per_account = parsed;
    file.save(&context.settings.config_path)?;
    println!("max_sessions_per_account = {parsed}");
    println!("saved = {}", context.settings.config_path.display());
    Ok(())
}

fn show(context: &RuntimeContext, as_json: bool) -> Result<()> {
    let settings = &context.settings;
    let overrides = context.model_overrides()?;
    if as_json {
        return output::json(&json!({
            "config": settings.config_path.display().to_string(),
            "max_subagents": settings.concurrency.max_subagents,
            "max_subagents_source": settings.max_subagents_source,
            "max_tasks": settings.concurrency.max_tasks,
            "max_sessions_per_account": settings.concurrency.max_sessions_per_account,
            "lanes_per_account": settings.concurrency.lanes_per_account,
            "fleet_live_cap": settings.concurrency.fleet_live_cap,
            "queue_ttl_seconds": settings.concurrency.queue_ttl_seconds,
            "models_config": context.model_config_path(),
            "models": overrides.effective()?,
        }));
    }
    println!("config = {}", settings.config_path.display());
    println!(
        "max_subagents = {} ({})",
        settings.concurrency.max_subagents, settings.max_subagents_source
    );
    println!("max_tasks = {}", settings.concurrency.max_tasks);
    println!(
        "max_sessions_per_account = {}",
        settings.concurrency.max_sessions_per_account
    );
    println!(
        "lanes_per_account = {}",
        settings.concurrency.lanes_per_account
    );
    println!(
        "fleet_live_cap = {}",
        settings
            .concurrency
            .fleet_live_cap
            .map_or_else(|| "none".to_owned(), |value| value.to_string())
    );
    println!(
        "queue_ttl_seconds = {}",
        settings.concurrency.queue_ttl_seconds
    );
    println!("models_config = {}", context.model_config_path().display());
    for model in overrides.effective()?.values() {
        println!(
            "model[{}] = {} ({})",
            model.engine, model.model, model.source
        );
    }
    Ok(())
}

fn show_models(context: &RuntimeContext, as_json: bool) -> Result<()> {
    let overrides = context.model_overrides()?;
    if as_json {
        return output::json(&json!({
            "config": context.model_config_path(),
            "models": overrides.effective()?,
        }));
    }
    println!("models_config = {}", context.model_config_path().display());
    for model in overrides.effective()?.values() {
        println!("{} = {} ({})", model.engine, model.model, model.source);
    }
    Ok(())
}

fn usage() -> Result<()> {
    Err(error::usage_error(anyhow::anyhow!(
        "usage: neomax config show [--json] | config models [--json] | config set max-subagents N | config set max-sessions-per-account N | config set-model ENGINE MODEL | config unset-model ENGINE"
    )))
}

#[cfg(test)]
mod tests {
    use neomax_core::{Engine, SettingsFile};

    use super::*;
    use crate::tests::fixture;

    #[test]
    fn persists_model_overrides_for_all_provider_names() {
        let fixture = fixture();
        for (engine, model) in [
            ("claude", "claude/custom"),
            ("codex", "codex/custom"),
            ("opencode", "local/provider/model"),
            ("kimi", "kimi/custom"),
            ("grok", "grok/custom"),
        ] {
            set_model_args(&fixture.context, &[engine.into(), model.into()]).unwrap();
        }
        let overrides = fixture.context.model_overrides().unwrap();
        assert_eq!(overrides.effective().unwrap().len(), 5);
        assert_eq!(
            overrides.get(Engine::Opencode),
            Some("local/provider/model")
        );
    }

    #[test]
    fn max_subagents_config_still_round_trips() {
        let fixture = fixture();
        set_max_subagents(&fixture.context, "13").unwrap();
        assert_eq!(
            SettingsFile::load(&fixture.context.settings.config_path)
                .unwrap()
                .concurrency
                .max_subagents,
            13
        );
    }
}
