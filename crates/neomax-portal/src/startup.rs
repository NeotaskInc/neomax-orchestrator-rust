use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::providers::scrub_provider_environment;

use crate::args::PortalArgs;

const USAGE_AGENT_BINARY_ENV: &str = "NEOMAX_USAGE_AGENT_BIN";
const PORTAL_USAGE_AGENT_DISABLE_ENV: &str = "NEOMAX_PORTAL_NO_USAGE_AGENT";
const USAGE_AGENT_COMMAND: &str = "ensure";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageAgentStartup {
    Disabled,
    Spawned { executable: PathBuf },
}

pub trait UsageAgentStarter: Send + Sync {
    fn spawn(&self, executable: &Path, state: &Path, cli: Option<&Path>) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct SystemUsageAgentStarter;

impl UsageAgentStarter for SystemUsageAgentStarter {
    fn spawn(&self, executable: &Path, state: &Path, cli: Option<&Path>) -> Result<()> {
        require_absolute("usage-agent executable", executable)?;
        require_absolute("usage-agent state", state)?;
        if let Some(cli) = cli {
            require_absolute("Neomax CLI executable", cli)?;
        }
        let mut command = Command::new(executable);
        scrub_provider_environment(&mut command);
        command
            .arg(USAGE_AGENT_COMMAND)
            .env("NEOMAX_HOME", state)
            .env(USAGE_AGENT_BINARY_ENV, executable)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        if let Some(cli) = cli {
            command.env("NEOMAX_CLI_BIN", cli);
        }
        command
            .spawn()
            .with_context(|| format!("start usage agent ensure ({})", executable.display()))?;
        Ok(())
    }
}

pub fn ensure_usage_agent(
    args: &PortalArgs,
    current_executable: &Path,
    starter: &dyn UsageAgentStarter,
) -> Result<UsageAgentStartup> {
    ensure_usage_agent_with_policy(
        args,
        current_executable,
        starter,
        env::var_os(PORTAL_USAGE_AGENT_DISABLE_ENV).is_some(),
    )
}

fn ensure_usage_agent_with_policy(
    args: &PortalArgs,
    current_executable: &Path,
    starter: &dyn UsageAgentStarter,
    disabled: bool,
) -> Result<UsageAgentStartup> {
    if disabled {
        return Ok(UsageAgentStartup::Disabled);
    }
    let executable = usage_agent_executable(current_executable)?;
    let current_dir = env::current_dir()?;
    let state = args
        .state
        .clone()
        .or_else(|| env::var_os("NEOMAX_HOME").map(PathBuf::from))
        .or_else(|| args.home.clone().map(|home| home.join(".neomax")))
        .or_else(|| {
            env::var_os("HOME")
                .or_else(|| env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".neomax"))
        })
        .ok_or_else(|| anyhow::anyhow!("HOME is not set; use --state PATH for the portal"))?;
    let state = absolute_root(state, &current_dir, "Neomax state")?;
    let cli = cli_executable(current_executable)?;
    starter.spawn(&executable, &state, cli.as_deref())?;
    Ok(UsageAgentStartup::Spawned { executable })
}

fn usage_agent_executable(current_executable: &Path) -> Result<PathBuf> {
    if let Some(value) = env::var_os(USAGE_AGENT_BINARY_ENV) {
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            bail!("{USAGE_AGENT_BINARY_ENV} must not be empty");
        }
        require_absolute(USAGE_AGENT_BINARY_ENV, &path)?;
        return Ok(path);
    }
    require_absolute("portal executable", current_executable)?;
    let parent = current_executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| anyhow::anyhow!("portal executable has no parent directory"))?;
    Ok(parent.join(if cfg!(windows) {
        "neomax-usage-agent.exe"
    } else {
        "neomax-usage-agent"
    }))
}

fn cli_executable(current_executable: &Path) -> Result<Option<PathBuf>> {
    if let Some(value) = env::var_os("NEOMAX_CLI_BIN") {
        let path = PathBuf::from(value);
        require_absolute("NEOMAX_CLI_BIN", &path)?;
        return Ok(Some(path));
    }
    require_absolute("portal executable", current_executable)?;
    Ok(current_executable
        .parent()
        .map(|parent| {
            parent.join(if cfg!(windows) {
                "neomax.exe"
            } else {
                "neomax"
            })
        })
        .filter(|path| path.is_file()))
}

fn absolute_root(path: PathBuf, current_dir: &Path, label: &str) -> Result<PathBuf> {
    if is_rooted_but_not_absolute(&path) {
        bail!("{label} must not be rooted without an absolute prefix");
    }
    if path.is_absolute() {
        return Ok(path);
    }
    require_absolute("current directory", current_dir)?;
    Ok(current_dir.join(path))
}

fn require_absolute(label: &str, path: &Path) -> Result<()> {
    if !path.is_absolute() || is_rooted_but_not_absolute(path) {
        bail!("{label} must be an absolute path");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
    struct FakeStarter {
        calls: Mutex<Vec<(PathBuf, PathBuf, Option<PathBuf>)>>,
    }

    impl UsageAgentStarter for FakeStarter {
        fn spawn(&self, executable: &Path, state: &Path, cli: Option<&Path>) -> Result<()> {
            self.calls.lock().unwrap().push((
                executable.to_owned(),
                state.to_owned(),
                cli.map(Path::to_owned),
            ));
            Ok(())
        }
    }

    fn fixture_current(temp: &TempDir) -> PathBuf {
        temp.path().join("bin").join(if cfg!(windows) {
            "neomax-portal.exe"
        } else {
            "neomax-portal"
        })
    }

    #[test]
    fn startup_is_injected_and_uses_the_relocated_state() {
        let temp = tempfile::tempdir().unwrap();
        let starter = FakeStarter::default();
        let state = temp.path().join("state");
        let home = temp.path().join("home");
        let args = PortalArgs::parse([
            "--state",
            state.to_str().unwrap(),
            "--home",
            home.to_str().unwrap(),
        ])
        .unwrap();
        let current = fixture_current(&temp);
        let expected_usage_agent = if cfg!(windows) {
            PathBuf::from("neomax-usage-agent.exe")
        } else {
            PathBuf::from("neomax-usage-agent")
        };
        let result = ensure_usage_agent(&args, &current, &starter).unwrap();
        assert!(matches!(result, UsageAgentStartup::Spawned { .. }));
        assert_eq!(
            starter.calls.lock().unwrap().as_slice(),
            &[(
                current.parent().unwrap().join(expected_usage_agent),
                state,
                None,
            )]
        );
    }

    #[test]
    fn portal_can_explicitly_disable_startup() {
        let temp = tempfile::tempdir().unwrap();
        let starter = FakeStarter::default();
        let state = temp.path().join("state");
        let args = PortalArgs::parse(["--state", state.to_str().unwrap()]).unwrap();
        let current = fixture_current(&temp);
        let result = ensure_usage_agent_with_policy(&args, &current, &starter, true).unwrap();
        assert_eq!(result, UsageAgentStartup::Disabled);
        assert!(starter.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn relative_roots_are_resolved_beneath_the_current_directory() {
        let temp = tempfile::tempdir().unwrap();
        let starter = FakeStarter::default();
        let args = PortalArgs::parse(["--state", "state", "--home", "home"]).unwrap();
        let current = fixture_current(&temp);
        let result = ensure_usage_agent_with_policy(&args, &current, &starter, false).unwrap();
        assert!(matches!(result, UsageAgentStartup::Spawned { .. }));
        assert_eq!(
            starter.calls.lock().unwrap()[0].1,
            env::current_dir().unwrap().join("state")
        );
    }

    #[cfg(windows)]
    #[test]
    fn partial_windows_roots_are_rejected_before_starting_the_agent() {
        let starter = FakeStarter::default();
        let current = Path::new(r"C:\fixture\bin\neomax-portal.exe");
        for value in [r"\state", r"C:state"] {
            let args = PortalArgs::parse(["--state", value]).unwrap();
            assert!(ensure_usage_agent_with_policy(&args, current, &starter, false).is_err());
        }
    }
}
