#[path = "cli/agent_normalize.rs"]
mod agent_normalize;
#[path = "cli/authorization.rs"]
mod authorization;
#[path = "cli/dispatch.rs"]
mod dispatch;
#[path = "cli/help.rs"]
mod help;

pub use authorization::authorize_agent_invocation;
#[cfg(test)]
pub(crate) use authorization::resolved_agent_command;
pub use dispatch::{execute, execute_install, execute_uninstall};
#[allow(unused_imports)]
pub use help::{help_text, is_help, is_version, print_help, print_version, version};

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
