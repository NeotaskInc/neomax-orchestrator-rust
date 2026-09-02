use std::env;

use anyhow::Result;

use neomax_portal::{
    args::{self, PortalArgs},
    server::PortalServer,
    source::FilesystemPortalSource,
};

fn main() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--version" | "-V"))
    {
        println!("neomax-portal {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "--help" | "-h"))
    {
        println!("{}", args::usage());
        return Ok(());
    }
    let args = PortalArgs::parse_env()?;
    let source = FilesystemPortalSource::from_args(&args)?;
    let current_executable = env::current_exe()?;
    if let Err(error) = neomax_portal::startup::ensure_usage_agent(
        &args,
        &current_executable,
        &neomax_portal::startup::SystemUsageAgentStarter,
    ) {
        eprintln!("[neomax] WARN portal could not ensure usage agent: {error}");
    }
    let server = PortalServer::new(args.bind, source).with_days(args.days);
    server.run()
}
