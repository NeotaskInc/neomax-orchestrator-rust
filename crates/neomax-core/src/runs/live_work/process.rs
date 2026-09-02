use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use std::time::Duration;

#[cfg(windows)]
mod windows;

#[cfg(unix)]
use crate::io::{LocalProcessRunner, ProcessRequest, ProcessRunner};
#[cfg(unix)]
use crate::providers::scrub_provider_process_request;
use crate::{Engine, Result};

#[cfg(unix)]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(unix)]
const PROCESS_LIST_LIMIT: usize = 512 * 1024;
#[cfg(unix)]
const PROCESS_DETAIL_LIMIT: usize = 128 * 1024;
const CLAUDE_CONFIG_ENV: &str = "CLAUDE_CONFIG_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmbientClaudeProcess {
    pub(crate) pid: u32,
    pub(crate) parent_pid: Option<u32>,
    pub(crate) profile: PathBuf,
}

pub(crate) trait ClaudeProcessSource: Send + Sync {
    fn processes(&self) -> Result<Vec<AmbientClaudeProcess>>;
}

pub(crate) struct SystemClaudeProcessSource {
    #[cfg(unix)]
    runner: Arc<dyn ProcessRunner>,
    #[cfg(windows)]
    inspector: Arc<dyn windows::WindowsProcessInspector>,
    home: Option<PathBuf>,
    current_dir: PathBuf,
}

impl Default for SystemClaudeProcessSource {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClaudeProcessSource {
    pub(crate) fn new() -> Self {
        #[cfg(unix)]
        {
            Self::with_runner(
                Arc::new(LocalProcessRunner::default()),
                default_home(),
                std::env::current_dir().unwrap_or_default(),
            )
        }

        #[cfg(windows)]
        {
            Self::with_inspector(
                Arc::new(windows::NativeWindowsProcessInspector),
                default_home(),
                std::env::current_dir().unwrap_or_default(),
            )
        }

        #[cfg(not(any(unix, windows)))]
        {
            Self {
                home: default_home(),
                current_dir: std::env::current_dir().unwrap_or_default(),
            }
        }
    }

    #[cfg(unix)]
    pub(crate) fn with_runner(
        runner: Arc<dyn ProcessRunner>,
        home: Option<PathBuf>,
        current_dir: PathBuf,
    ) -> Self {
        Self {
            runner,
            home,
            current_dir,
        }
    }

    #[cfg(windows)]
    pub(crate) fn with_inspector(
        inspector: Arc<dyn windows::WindowsProcessInspector>,
        home: Option<PathBuf>,
        current_dir: PathBuf,
    ) -> Self {
        Self {
            inspector,
            home,
            current_dir,
        }
    }

    #[cfg(unix)]
    fn discover(&self) -> Result<Vec<AmbientClaudeProcess>> {
        let uid = current_uid();
        let rows = self.capture(["-U".into(), uid, "-o".into(), "pid=,ppid=,comm=".into()])?;
        let candidates = parse_claude_rows(&rows);
        let mut processes = Vec::with_capacity(candidates.len());
        for row in candidates {
            let pid = row.pid.to_string();
            let Ok(command) = self.capture([
                "eww".into(),
                "-o".into(),
                "command=".into(),
                "-p".into(),
                pid,
            ]) else {
                continue;
            };
            let Some(profile) =
                profile_from_command(&command, self.home.as_deref(), &self.current_dir)
            else {
                continue;
            };
            processes.push(AmbientClaudeProcess {
                pid: row.pid,
                parent_pid: row.parent_pid,
                profile,
            });
        }
        Ok(processes)
    }

    #[cfg(windows)]
    fn discover(&self) -> Result<Vec<AmbientClaudeProcess>> {
        let processes = self
            .inspector
            .processes()?
            .into_iter()
            .filter_map(|process| {
                windows::is_claude_process(&process)
                    .then(|| {
                        process
                            .config_dir
                            .as_deref()
                            .and_then(|value| profile_from_value(value, &self.current_dir))
                            .or_else(|| {
                                profile_from_command(
                                    process.command_line.as_deref().unwrap_or_default(),
                                    self.home.as_deref(),
                                    &self.current_dir,
                                )
                            })
                    })
                    .flatten()
                    .map(|profile| AmbientClaudeProcess {
                        pid: process.pid,
                        parent_pid: process.parent_pid,
                        profile,
                    })
            })
            .collect::<Vec<_>>();
        Ok(processes)
    }

    #[cfg(not(any(unix, windows)))]
    fn discover(&self) -> Result<Vec<AmbientClaudeProcess>> {
        let _ = (&self.home, &self.current_dir);
        Ok(Vec::new())
    }

    #[cfg(unix)]
    fn capture<I>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = String>,
    {
        let request = ProcessRequest::new("ps")
            .args(args)
            .timeout(PROCESS_TIMEOUT)
            .stdout_limit(PROCESS_LIST_LIMIT)
            .stderr_limit(PROCESS_DETAIL_LIMIT);
        let request = scrub_provider_process_request(request);
        let output = self.runner.capture(&request).map_err(crate::Error::from)?;
        if !output.success || output.timed_out || output.stdout_truncated || output.stderr_truncated
        {
            return Err(crate::Error::Message(
                "unable to inspect Claude processes".into(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl ClaudeProcessSource for SystemClaudeProcessSource {
    fn processes(&self) -> Result<Vec<AmbientClaudeProcess>> {
        self.discover()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(unix)]
struct ProcessRow {
    pid: u32,
    parent_pid: Option<u32>,
}

#[cfg(unix)]
fn parse_claude_rows(output: &str) -> Vec<ProcessRow> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let parent_pid = fields.next().and_then(|value| value.parse().ok());
            let command = fields.next()?;
            let name = Path::new(command).file_name()?.to_str()?;
            name.eq_ignore_ascii_case("claude")
                .then_some(ProcessRow { pid, parent_pid })
        })
        .collect()
}

fn profile_from_command(command: &str, home: Option<&Path>, current_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    let configured = windows::profile_environment_value(command, CLAUDE_CONFIG_ENV)
        .or_else(|| environment_value(command, CLAUDE_CONFIG_ENV));
    #[cfg(not(windows))]
    let configured = environment_value(command, CLAUDE_CONFIG_ENV);
    let profile = configured
        .or_else(|| home.map(|path| path.join(".claude").to_string_lossy().into_owned()))?;
    profile_from_value(&profile, current_dir)
}

fn profile_from_value(value: &str, current_dir: &Path) -> Option<PathBuf> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    let profile = PathBuf::from(value);
    if crate::io::is_rooted_but_not_absolute(&profile) {
        return None;
    }
    if profile.is_absolute() {
        Some(profile)
    } else {
        if crate::io::is_rooted_but_not_absolute(current_dir) {
            return None;
        }
        Some(current_dir.join(profile))
    }
}

fn environment_value(command: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=");
    let mut value = None;
    for (offset, _) in command.match_indices(&marker) {
        if offset == 0 || command.as_bytes()[offset - 1].is_ascii_whitespace() {
            value = Some(&command[offset + marker.len()..]);
        }
    }
    let value = value?;
    let end = next_environment_boundary(value).unwrap_or(value.len());
    let value = value[..end].trim();
    (!value.is_empty() && !value.chars().any(char::is_control)).then(|| value.to_owned())
}

fn next_environment_boundary(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    for index in 1..bytes.len() {
        if !bytes[index].is_ascii_whitespace() {
            continue;
        }
        let mut start = index;
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_uppercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'_')
        {
            end += 1;
        }
        if end > start && bytes.get(end) == Some(&b'=') {
            return Some(index);
        }
    }
    None
}

pub(crate) fn add_ambient_counts(
    counts: &mut BTreeMap<(Engine, PathBuf), u32>,
    registered_claude_pids: &BTreeSet<u32>,
    processes: &[AmbientClaudeProcess],
) {
    let process_pids = processes
        .iter()
        .map(|process| process.pid)
        .collect::<BTreeSet<_>>();
    let mut counted_pids = BTreeSet::new();
    for process in processes {
        if !counted_pids.insert(process.pid)
            || registered_claude_pids.contains(&process.pid)
            || process.parent_pid.is_some_and(|parent| {
                registered_claude_pids.contains(&parent) || process_pids.contains(&parent)
            })
        {
            continue;
        }
        *counts
            .entry((Engine::Claude, process.profile.clone()))
            .or_default() += 1;
    }
}

fn default_home() -> Option<PathBuf> {
    crate::runtime::RuntimeEnvironment::process().home_dir()
}

#[cfg(unix)]
fn current_uid() -> String {
    // SAFETY: getuid has no preconditions and only reads the calling process identity.
    unsafe { libc::getuid() }.to_string()
}

#[cfg(test)]
mod tests {
    #[cfg(any(unix, windows))]
    use std::sync::Arc;

    #[cfg(unix)]
    use crate::io::{ProcessOutput, ProcessRequest};

    #[cfg(windows)]
    use super::windows::{WindowsProcessInfo, WindowsProcessInspector};

    use super::*;

    #[cfg(unix)]
    struct FixtureRunner;

    #[cfg(unix)]
    impl ProcessRunner for FixtureRunner {
        fn capture(&self, request: &ProcessRequest) -> crate::io::Result<ProcessOutput> {
            let args = request
                .args
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let stdout = if args.first().is_some_and(|value| value == "eww") {
                match args.last().map(String::as_str) {
                    Some("10") => "claude CLAUDE_CONFIG_DIR=/profiles/a OTHER=one".into(),
                    Some("11") => "claude CLAUDE_CONFIG_DIR=/profiles/a".into(),
                    Some("12") => "claude PATH=/usr/bin".into(),
                    _ => String::new(),
                }
            } else {
                "10 1 claude\n11 10 claude\n12 1 claude\n13 1 neomax\n".into()
            };
            Ok(ProcessOutput {
                status_code: Some(0),
                success: true,
                stdout: stdout.into_bytes(),
                stderr: Vec::new(),
                timed_out: false,
                stdout_truncated: false,
                stderr_truncated: false,
            })
        }
    }

    #[cfg(unix)]
    #[test]
    fn system_source_uses_injected_process_listing_without_shelling_out() {
        let source = SystemClaudeProcessSource::with_runner(
            Arc::new(FixtureRunner),
            Some(PathBuf::from("/home/tester")),
            PathBuf::from("/workspace"),
        );

        let processes = source.processes().unwrap();

        assert_eq!(processes.len(), 3);
        assert_eq!(processes[0].pid, 10);
        assert_eq!(processes[0].profile, PathBuf::from("/profiles/a"));
        assert_eq!(processes[2].profile, PathBuf::from("/home/tester/.claude"));
    }

    #[cfg(unix)]
    #[test]
    fn process_rows_require_an_exact_claude_executable_name() {
        let rows = parse_claude_rows(
            "10 1 claude\n11 10 /usr/local/bin/claude\n12 1 claude-helper\n13 1 neomax\n",
        );
        assert_eq!(
            rows,
            vec![
                ProcessRow {
                    pid: 10,
                    parent_pid: Some(1)
                },
                ProcessRow {
                    pid: 11,
                    parent_pid: Some(10)
                }
            ]
        );
    }

    #[test]
    fn profile_environment_supports_spaces_and_defaults_to_claude_home() {
        #[cfg(not(windows))]
        let (command, home, current_dir, expected) = (
            "claude CLAUDE_CONFIG_DIR=/profiles/a b OTHER=value",
            Path::new("/home/tester"),
            Path::new("/workspace"),
            PathBuf::from("/profiles/a b"),
        );
        #[cfg(windows)]
        let (command, home, current_dir, expected) = (
            r#"claude CLAUDE_CONFIG_DIR="C:\profiles\a b" OTHER=value"#,
            Path::new(r"C:\home\tester"),
            Path::new(r"C:\workspace"),
            PathBuf::from(r"C:\profiles\a b"),
        );
        assert_eq!(
            profile_from_command(command, Some(home), current_dir),
            Some(expected)
        );
        assert_eq!(
            profile_from_command("claude PATH=/usr/bin", Some(home), current_dir),
            Some(home.join(".claude"))
        );
    }

    #[test]
    fn command_argument_with_the_config_marker_is_not_used_when_env_marker_follows() {
        assert_eq!(
            environment_value(
                "claude --prompt CLAUDE_CONFIG_DIR=/argument CLAUDE_CONFIG_DIR=/profile",
                CLAUDE_CONFIG_ENV,
            ),
            Some("/profile".into())
        );
    }

    #[cfg(windows)]
    #[test]
    fn rooted_or_drive_relative_profile_values_are_rejected() {
        let current_dir = Path::new(r"C:\workspace");
        assert_eq!(
            profile_from_value(r"\profiles\secondary", current_dir),
            None
        );
        assert_eq!(
            profile_from_value(r"C:profiles\secondary", current_dir),
            None
        );
    }

    #[cfg(windows)]
    struct FixtureInspector {
        processes: Vec<WindowsProcessInfo>,
    }

    #[cfg(windows)]
    impl WindowsProcessInspector for FixtureInspector {
        fn processes(&self) -> Result<Vec<WindowsProcessInfo>> {
            Ok(self.processes.clone())
        }
    }

    #[cfg(windows)]
    #[test]
    fn source_accepts_injected_filtered_process_inventory() {
        let source = SystemClaudeProcessSource::with_inspector(
            Arc::new(FixtureInspector {
                processes: vec![
                    WindowsProcessInfo {
                        pid: 10,
                        parent_pid: Some(1),
                        image_path: r"C:\bin\claude.exe".into(),
                        command_line: None,
                        config_dir: None,
                    },
                    WindowsProcessInfo {
                        pid: 11,
                        parent_pid: Some(1),
                        image_path: r"C:\node\node.exe".into(),
                        command_line: Some(
                            r#"node "C:\node_modules\@anthropic-ai\claude-code\cli.js" CLAUDE_CONFIG_DIR="C:\profiles\secondary""#
                                .into(),
                        ),
                        config_dir: Some(r"C:\profiles\secondary".into()),
                    },
                    WindowsProcessInfo {
                        pid: 12,
                        parent_pid: Some(1),
                        image_path: r"C:\bin\claude-helper.exe".into(),
                        command_line: None,
                        config_dir: None,
                    },
                ],
            }),
            Some(PathBuf::from(r"C:\Users\tester")),
            PathBuf::from(r"C:\workspace"),
        );

        let processes = source.processes().unwrap();

        assert_eq!(processes.len(), 2);
        assert_eq!(
            processes[0].profile,
            PathBuf::from(r"C:\Users\tester\.claude")
        );
        assert_eq!(
            processes[1].profile,
            PathBuf::from(r#"C:\profiles\secondary"#)
        );
    }
}
