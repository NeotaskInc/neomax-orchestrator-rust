use std::env;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::registry::{OrchestratorRecord, OrchestratorStore};
use neomax_core::runs::SystemProcessProbe;
use neomax_core::shepherd::GitInspector;
use serde_json::json;

use super::super::args;
use crate::context::RuntimeContext;
use crate::output;

const VALUE_FLAGS: &[&str] = &["--repo", "--base"];
const SWITCH_FLAGS: &[&str] = &["--json"];

pub(crate) fn run(context: &RuntimeContext, args: &[String]) -> Result<()> {
    let parsed = args::parse(args, VALUE_FLAGS, SWITCH_FLAGS)?;
    if parsed.positionals.len() > 1 {
        bail!("premerge accepts at most one repository path");
    }
    if parsed.value("--repo").is_some() && !parsed.positionals.is_empty() {
        bail!("premerge accepts either a repository path or --repo, not both");
    }
    let requested_repository = match parsed.value("--repo") {
        Some(value) => resolve_repository_arg(context, value)?,
        None => parsed
            .positionals
            .first()
            .map(|value| resolve_repository_arg(context, value))
            .transpose()?
            .unwrap_or_else(|| context.cwd.clone()),
    };
    validate_absolute_path(&requested_repository, "repository")?;
    let base = parsed.value("--base").unwrap_or("main");
    let inspector = GitInspector::new();
    let repository = inspector.repository_root(&requested_repository)?;
    validate_absolute_path(&repository, "repository")?;

    let (fetched, fetch_failed) = match inspector.fetch_origin(&repository, base) {
        Ok(output) => (output.success, !output.success),
        Err(_) => (false, true),
    };
    let (behind, behind_failed) = if fetched {
        match inspector.commits_behind_origin(&repository, base) {
            Ok(count) => (count, false),
            Err(_) => (0, true),
        }
    } else {
        (0, false)
    };
    let report = json!({
        "repo": repository,
        "base": base,
        "main_moved": behind > 0,
        "fetched": fetched,
        "behind": behind,
        "other_orchestrators": live_orchestrator_rows(context, &repository)?,
    });
    if parsed.has("--json") {
        return output::json(&report);
    }
    if report["main_moved"].as_bool().unwrap_or(false) {
        println!(
            "origin/{base} moved - you are {behind} commit(s) behind. Pull or rebase before merging."
        );
    } else if fetch_failed || behind_failed {
        println!(
            "origin/{base} could not be verified (fetch failed); inspect it manually before merging."
        );
    } else {
        println!("origin/{base} has not moved since your local base.");
    }
    let others = report["other_orchestrators"].as_array().map_or(0, Vec::len);
    if others == 0 {
        println!("no other live orchestrator is working on this repo.");
    } else {
        println!(
            "warning: {others} other live orchestrator(s) are working on this repo; coordinate the merge."
        );
        for other in report["other_orchestrators"]
            .as_array()
            .into_iter()
            .flatten()
        {
            println!(
                "  {} account {} project {} branch {}/",
                other["engine"].as_str().unwrap_or("-"),
                account_display(&other["account"]),
                other["project"].as_str().unwrap_or("-"),
                other["branch_prefix"].as_str().unwrap_or("-")
            );
        }
    }
    Ok(())
}

fn live_orchestrator_rows(
    context: &RuntimeContext,
    repository: &Path,
) -> Result<Vec<serde_json::Value>> {
    let current_session = env::var("NEOMAX_ORCH_SESSION").ok();
    let records = OrchestratorStore::new(&context.paths.orchestrators)
        .all(&SystemProcessProbe, context.now)?;
    Ok(matching_live_orchestrators(
        &context.cwd,
        repository,
        current_session.as_deref(),
        &records,
    ))
}

pub(crate) fn matching_live_orchestrators(
    context_cwd: &Path,
    repository: &Path,
    current_session: Option<&str>,
    records: &[OrchestratorRecord],
) -> Vec<serde_json::Value> {
    records
        .iter()
        .filter(|record| record.live)
        .filter(|record| current_session.is_none_or(|session| session != record.session))
        .filter(|record| paths_overlap(context_cwd, repository, &record.cwd))
        .map(|record| orchestrator_row(record.clone()))
        .collect()
}

fn orchestrator_row(record: OrchestratorRecord) -> serde_json::Value {
    json!({
        "engine": record.engine,
        "account": record.account,
        "project": record.project,
        "branch_prefix": record.branch_prefix,
    })
}

fn paths_overlap(context_cwd: &Path, repository: &Path, orchestrator_cwd: &Path) -> bool {
    if !is_absolute_safe(context_cwd)
        || !is_absolute_safe(repository)
        || !is_absolute_safe(orchestrator_cwd)
    {
        return false;
    }
    repository.starts_with(orchestrator_cwd) || orchestrator_cwd.starts_with(repository)
}

fn resolve_repository_arg(context: &RuntimeContext, value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    validate_path(path, "repository")?;
    validate_absolute_path(&context.cwd, "working directory")?;
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        context.cwd.join(path)
    };
    validate_absolute_path(&resolved, "repository")?;
    Ok(resolved)
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<()> {
    validate_path(path, label)?;
    if !path.is_absolute() {
        bail!("{label} must be absolute: {}", path.display());
    }
    Ok(())
}

fn validate_path(path: &Path, label: &str) -> Result<()> {
    if is_rooted_but_not_absolute(path) {
        bail!(
            "{label} must not be rooted without an absolute prefix: {}",
            path.display()
        );
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!(
            "{label} cannot contain parent-directory traversal: {}",
            path.display()
        );
    }
    Ok(())
}

fn is_absolute_safe(path: &Path) -> bool {
    path.is_absolute()
        && !is_rooted_but_not_absolute(path)
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn account_display(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|account| account.to_string()))
        .unwrap_or_else(|| "-".into())
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn overlap_ignores_non_absolute_or_traversing_persisted_paths() {
        assert!(!paths_overlap(
            Path::new("/workspace"),
            Path::new("/workspace/repo"),
            Path::new("../repo")
        ));
        assert!(!paths_overlap(
            Path::new("/workspace"),
            Path::new("/workspace/repo"),
            Path::new("relative/repo")
        ));
    }

    #[test]
    fn repository_arguments_allow_safe_relative_labels() {
        let temp = tempfile::tempdir().expect("temporary root");
        let settings = neomax_core::EffectiveSettings::resolve(
            neomax_core::SettingsFile::default(),
            temp.path().join("config.toml"),
            &std::collections::BTreeMap::new(),
        )
        .expect("settings");
        let context = RuntimeContext::for_test(
            neomax_core::StatePaths::new(temp.path().join("home"), temp.path().join("state")),
            settings,
            temp.path().to_path_buf(),
            1,
            Default::default(),
            None,
        );
        assert_eq!(
            resolve_repository_arg(&context, "repo").expect("relative repository"),
            temp.path().join("repo")
        );
        assert!(resolve_repository_arg(&context, "../repo").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn repository_arguments_reject_windows_partial_roots() {
        let temp = tempfile::tempdir().expect("temporary root");
        let settings = neomax_core::EffectiveSettings::resolve(
            neomax_core::SettingsFile::default(),
            temp.path().join("config.toml"),
            &std::collections::BTreeMap::new(),
        )
        .expect("settings");
        let context = RuntimeContext::for_test(
            neomax_core::StatePaths::new(temp.path().join("home"), temp.path().join("state")),
            settings,
            temp.path().to_path_buf(),
            1,
            Default::default(),
            None,
        );
        for value in [r"\repo", r"C:repo"] {
            assert!(resolve_repository_arg(&context, value).is_err());
        }
    }
}
