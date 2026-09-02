mod files;
mod install;
mod manifest;
mod package;
mod paths;
mod transaction;
mod types;
mod uninstall;
mod workflows;
#[cfg(windows)]
mod windows;

pub use install::install;
pub use paths::{InstallPaths, PackageRoot};
pub use types::{InstallOptions, InstallReport, UninstallOptions, UninstallReport};
pub use uninstall::uninstall;

pub use workflows::ensure_profile_workflows;

#[cfg(test)]
mod tests;
