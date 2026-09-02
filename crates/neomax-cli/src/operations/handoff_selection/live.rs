use std::path::Path;

use anyhow::{Result, bail};
use neomax_core::Engine;
use neomax_core::accounts::AccountSnapshot;
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::registry::OrchestratorRecord;

use super::super::options::HandoffOptions;
use super::policy::select_with_profile;
use super::types::HandoffSelection;
use crate::context::RuntimeContext;

pub(crate) fn select_live_orchestrator(
    options: &HandoffOptions,
    context: &RuntimeContext,
    accounts: &[AccountSnapshot],
    record: &OrchestratorRecord,
) -> Result<(HandoffOptions, HandoffSelection)> {
    let account_path = account_directory(&record.account_dir)?;

    if is_rooted_but_not_absolute(&record.cwd) {
        bail!(
            "live orchestrator working directory must not be rooted without an absolute prefix: {}",
            record.cwd.display()
        );
    }
    if is_rooted_but_not_absolute(&context.paths.home) {
        bail!(
            "live orchestrator profile home must not be rooted without an absolute prefix: {}",
            context.paths.home.display()
        );
    }
    let account_dir = account_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(record.account_dir.as_str());
    let account_number = record.account.map(|account| account.to_string());
    let profile = accounts
        .iter()
        .filter(|account| !is_rooted_but_not_absolute(&account.profile))
        .find(|account| {
            account.engine == record.engine
                && (account
                    .profile
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name == account_dir)
                    || account_number
                        .as_deref()
                        .is_some_and(|number| account.account == number))
        })
        .map(|account| account.profile.clone())
        .unwrap_or_else(|| {
            if record.account_dir.is_empty() {
                context.paths.home.join(match record.engine {
                    Engine::Claude => ".claude",
                    Engine::Codex => ".codex",
                    Engine::Opencode => ".opencode",
                    Engine::Kimi => ".kimi-code",
                    Engine::Grok => ".grok",
                })
            } else {
                context.paths.home.join(account_path)
            }
        });
    let worker_scope = record
        .extra
        .get("worker_scope")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let options = options.for_live_orchestrator(record, worker_scope);
    let selection = select_with_profile(
        &options,
        context,
        accounts,
        profile,
        &options.environment.values,
    )?;
    Ok((options, selection))
}

fn account_directory(value: &str) -> Result<&Path> {
    let account_path = Path::new(value);
    if is_rooted_but_not_absolute(account_path) {
        bail!(
            "live orchestrator account profile must not be rooted without an absolute prefix: {}",
            value
        );
    }
    let mut components = account_path.components();
    if !value.is_empty()
        && (!matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some())
    {
        bail!(
            "live orchestrator account profile must be a single directory name: {}",
            value
        );
    }
    Ok(account_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use neomax_core::orchestration::registry::OrchestratorRecord;
    #[cfg(windows)]
    use neomax_core::runs::ProbeState;

    #[cfg(windows)]
    #[test]
    fn rejects_windows_partial_root_account_directories_before_home_joining() {
        let fixture = crate::tests::fixture();
        for account_dir in [r"\rooted", r"C:drive-relative"] {
            let record = OrchestratorRecord {
                session: "session".into(),
                pid: None,
                engine: Engine::Claude,
                account: Some(1),
                account_dir: account_dir.into(),
                project: None,
                branch_prefix: None,
                cwd: std::path::PathBuf::from(r"C:\workspace"),
                model: "claude-fable-5[1m]".into(),
                reserved: false,
                started: 0,
                last_seen: 0,
                live: false,
                process_state: ProbeState::Unknown,
                extra: Default::default(),
            };
            let error = select_live_orchestrator(
                &crate::operations::handoff::options::HandoffOptions {
                    engine: Engine::Claude,
                    source_account: None,
                    target_selectors: Vec::new(),
                    reason: "quota".into(),
                    reason_explicit: true,
                    cwd: fixture.context.cwd.clone(),
                    kickoff: None,
                    worker_scope: None,
                    model_overrides: Default::default(),
                    environment: Default::default(),
                    headless: true,
                    check: false,
                    dry_run: true,
                    json: false,
                    run_id: None,
                    session: None,
                    interactive_only: false,
                },
                &fixture.context,
                &[],
                &record,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
    }

    #[test]
    fn rejects_persisted_account_directory_traversal_before_home_joining() {
        for account_dir in ["../outside", "nested/profile"] {
            let error = account_directory(account_dir).unwrap_err();
            assert!(error.to_string().contains("single directory name"));
        }
        #[cfg(not(windows))]
        {
            let error = account_directory("/absolute").unwrap_err();
            assert!(error.to_string().contains("single directory name"));
        }
        #[cfg(windows)]
        {
            let error = account_directory(r"\absolute").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
        assert_eq!(
            account_directory(".opencode").unwrap(),
            Path::new(".opencode")
        );
        assert_eq!(account_directory("").unwrap(), Path::new(""));
    }
}
