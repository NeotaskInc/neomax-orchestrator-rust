use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::handoff::{
    HandoffTargetRequest, TargetPolicy, check_result, current_profile, select_target,
};

use super::super::options::HandoffOptions;
use super::source::{reset_label, source_account};
use super::types::{HandoffSelection, context_time};
use crate::context::RuntimeContext;

pub(crate) fn select(
    options: &HandoffOptions,
    context: &RuntimeContext,
    accounts: &[AccountSnapshot],
) -> Result<HandoffSelection> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    select_with_environment(options, context, accounts, &environment)
}

pub(crate) fn select_with_environment(
    options: &HandoffOptions,
    context: &RuntimeContext,
    accounts: &[AccountSnapshot],
    environment: &BTreeMap<String, String>,
) -> Result<HandoffSelection> {
    let current_profile = current_profile(
        options.engine,
        environment,
        &context.paths.home,
        &context.cwd,
    );
    select_with_profile(options, context, accounts, current_profile, environment)
}

pub(crate) fn select_with_profile(
    options: &HandoffOptions,
    context: &RuntimeContext,
    accounts: &[AccountSnapshot],
    current_profile: PathBuf,
    environment: &BTreeMap<String, String>,
) -> Result<HandoffSelection> {
    let current_profile = canonical_profile(current_profile)?;
    let accounts = accounts
        .iter()
        .filter(|account| !is_rooted_but_not_absolute(&account.profile))
        .cloned()
        .collect::<Vec<_>>();
    let source = source_account(
        options.engine,
        &current_profile,
        options.source_account.as_deref(),
        &accounts,
        environment,
        &context.paths.home,
    )?;
    let policy = TargetPolicy {
        allow_reserved: true,
        ..TargetPolicy::default()
    };
    let request = HandoffTargetRequest {
        accounts: &accounts,
        engine: options.engine,
        current_profile: &current_profile,
        selectors: &options.target_selectors,
        now: context_time(context),
        policy: &policy,
    };
    let target = if accounts.is_empty() {
        None
    } else {
        match select_target(&request) {
            Ok(target) => Some(target),
            Err(_error) if options.check => None,
            Err(error) => return Err(error.into()),
        }
    };
    let target_account = target
        .as_ref()
        .map(|selection| selection.account.account.clone());
    let target_reset = target
        .as_ref()
        .and_then(|selection| reset_label(selection.account.weekly_reset_at, request.now));
    let check = check_result(
        options.engine,
        source.account.clone(),
        source.five_hour_at(request.now),
        source.weekly_at(request.now),
        target_account,
        target_reset,
        None,
    );
    Ok(HandoffSelection {
        engine: options.engine,
        current_profile,
        source,
        target,
        check,
    })
}

fn canonical_profile(profile: PathBuf) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(&profile) {
        anyhow::bail!(
            "handoff source profile must not be rooted without an absolute prefix: {}",
            profile.display()
        );
    }
    Ok(std::fs::canonicalize(&profile).unwrap_or(profile))
}
