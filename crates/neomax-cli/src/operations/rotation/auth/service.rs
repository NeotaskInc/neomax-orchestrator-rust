use anyhow::{Context, Result, bail};
use neomax_core::Engine;
use neomax_core::orchestration::auth::RotationService;
use neomax_core::orchestration::commands::Launcher;

use super::options::{AuthOptions, launcher_engine, selectors_need_discovery};
use super::profiles::{ensure_rotation_profile, resolve_profile, rotation_paths};
use super::report::{from_effects, print_log, print_report};
use crate::context::RuntimeContext;
use crate::error;
use crate::operations::rotation::render;

pub(crate) fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    let options = error::usage(AuthOptions::parse(args))?;
    if options.log {
        return print_log(launcher, &options, context);
    }
    if options.destination.is_none() && options.restore.is_none() {
        return render::no_op(
            "rotate-auth",
            args,
            "no destination or restore target supplied; credentials were not touched",
        );
    }
    let engine = options
        .engine
        .or_else(|| launcher_engine(launcher))
        .or_else(|| std::env::var("NEOMAX_ROLE").ok()?.parse().ok())
        .unwrap_or(Engine::Claude);
    if let (Some(requested), Some(pinned)) = (options.engine, launcher_engine(launcher)) {
        if requested != pinned {
            bail!(
                "{} is pinned to {pinned}; rotate-auth cannot target {requested}",
                crate::launch::invocation_name(launcher)
            );
        }
    }
    let runtime = if selectors_need_discovery(&options)
        || options.destination.is_some()
        || options.source.is_some()
        || options.restore.is_some()
    {
        Some(context.provider_runtime()?)
    } else {
        None
    };
    let service = RotationService::filesystem(rotation_paths(context));
    let timestamp = context.now;
    if let Some(selector) = options.restore.as_deref() {
        let destination = resolve_profile(runtime.as_ref(), engine, selector, &context.paths.home)?;
        let effects = service.restore(
            engine,
            &destination,
            None,
            timestamp,
            options
                .reason
                .clone()
                .or_else(|| Some("manual restore".into())),
        )?;
        return print_report(
            &options,
            from_effects(engine, "restore", effects, Some(destination), None),
        );
    }
    let destination_selector = options
        .destination
        .as_deref()
        .context("rotate-auth requires a destination profile")?;
    let source_selector = options
        .source
        .as_deref()
        .context("rotate-auth requires --from SOURCE for copy or swap")?;
    let destination = resolve_profile(
        runtime.as_ref(),
        engine,
        destination_selector,
        &context.paths.home,
    )?;
    let source = resolve_profile(
        runtime.as_ref(),
        engine,
        source_selector,
        &context.paths.home,
    )?;
    ensure_rotation_profile(runtime.as_ref(), engine, &source, "source")?;
    if options.swap {
        ensure_rotation_profile(runtime.as_ref(), engine, &destination, "destination")?;
    }
    let reason = options
        .reason
        .clone()
        .or_else(|| Some("manual rotation".into()));
    let (operation, effects) = if options.swap {
        (
            "swap",
            service.swap(engine, &destination, &source, timestamp, reason)?,
        )
    } else {
        (
            "copy",
            service.copy(engine, &destination, &source, timestamp, reason)?,
        )
    };
    print_report(
        &options,
        from_effects(engine, operation, effects, Some(destination), Some(source)),
    )
}
