//! Cross-platform runtime inputs shared by discovery, launch, and installation.

mod environment;
mod executable;
mod paths;
mod platform;

#[cfg(test)]
mod tests;

pub use environment::{RuntimeEnvironment, process_command};
#[cfg(any(test, windows))]
pub(crate) use executable::quote_cmd_argument;
pub use executable::{ResolvedProviderExecutable, resolve_provider_executable};
pub use paths::{
    native_home, opencode_config_dir, opencode_data_dir, opencode_data_root, resolve_path,
    safe_child_environment, temp_dir,
};
pub use platform::{RuntimePlatform, WINDOWS_CHILD_ENVIRONMENT};
