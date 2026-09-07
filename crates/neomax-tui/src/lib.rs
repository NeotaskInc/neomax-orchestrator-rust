mod app;
mod data;
mod terminal;
mod view;

use std::io::{self, IsTerminal};
use std::path::PathBuf;

use anyhow::{Result, bail};
use neomax_core::providers::runtime::ProviderRuntime;

pub struct Options {
    pub home: PathBuf,
    pub state: PathBuf,
    pub cwd: PathBuf,
    pub executable: PathBuf,
    pub runtime: ProviderRuntime,
}

pub fn run(options: Options, args: &[String]) -> Result<()> {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "neomax tui\n\nLaunch, Chat, Fleet, Tasks, Accounts and Usage.\nLeft/right or Tab changes page. Enter opens or confirms. ? shows keys.\nCtrl+] returns from Chat input. s changes the header spend range.\nBrowsing does not launch a provider. Launch requires confirmation."
        );
        return Ok(());
    }
    if !args.is_empty() {
        bail!("neomax tui takes no options; use --help for keyboard controls");
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("neomax tui needs an interactive terminal");
    }
    app::run(options)
}
