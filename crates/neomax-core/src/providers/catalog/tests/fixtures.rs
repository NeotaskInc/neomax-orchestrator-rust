use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::super::CommandRunner;
use super::super::{CommandOutput, DiscoveryCommand, FileSystem, MapEnvironment};

#[derive(Default)]
pub struct FixtureFs {
    pub files: BTreeMap<PathBuf, Vec<u8>>,
    pub dirs: Vec<PathBuf>,
}

impl FixtureFs {
    pub fn dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.dirs.push(path.into());
        self
    }

    pub fn file(mut self, path: impl Into<PathBuf>, contents: impl AsRef<[u8]>) -> Self {
        self.files.insert(path.into(), contents.as_ref().to_vec());
        self
    }
}

impl FileSystem for FixtureFs {
    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.dirs.iter().any(|item| item == path)
    }

    fn read(&self, path: &Path) -> crate::Result<Option<Vec<u8>>> {
        Ok(self.files.get(path).cloned())
    }

    fn children(&self, path: &Path) -> crate::Result<Vec<PathBuf>> {
        let mut children = self
            .dirs
            .iter()
            .chain(self.files.keys())
            .filter(|item| item.parent() == Some(path))
            .map(|item| item.to_path_buf())
            .collect::<Vec<_>>();
        children.sort();
        children.dedup();
        Ok(children)
    }
}

#[derive(Default)]
pub struct FixtureCommands {
    pub outputs: BTreeMap<String, CommandOutput>,
    pub seen: Arc<Mutex<Vec<DiscoveryCommand>>>,
}

impl FixtureCommands {
    pub fn output(mut self, program: impl Into<String>, output: CommandOutput) -> Self {
        self.outputs.insert(program.into(), output);
        self
    }
}

impl CommandRunner for FixtureCommands {
    fn run(&self, command: &DiscoveryCommand) -> crate::Result<CommandOutput> {
        self.seen.lock().unwrap().push(command.clone());
        Ok(self
            .outputs
            .get(&command.program)
            .cloned()
            .unwrap_or(CommandOutput {
                success: false,
                stdout: Vec::new(),
                timed_out: false,
                truncated: false,
            }))
    }
}

pub fn environment(home: &Path) -> MapEnvironment {
    let fixture_bin = home.join("bin");
    MapEnvironment::new([
        (
            "PATH".into(),
            fixture_bin.to_string_lossy().into_owned(),
        ),
        ("NEOMAX_CLAUDE_BIN".into(), "claude-fixture".into()),
        ("NEOMAX_OPENCODE_BIN".into(), "opencode-fixture".into()),
    ])
    .with_home(home)
    .with_current_dir(home)
}

pub fn opencode_auth_path(home: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        home.join("AppData")
            .join("Local")
            .join("opencode")
            .join("auth.json")
    }
    #[cfg(not(windows))]
    {
        home.join(".local")
            .join("share")
            .join("opencode")
            .join("auth.json")
    }
}
