#[path = "auth/options.rs"]
mod options;
#[path = "auth/profiles.rs"]
mod profiles;
#[path = "auth/report.rs"]
mod report;
#[path = "auth/service.rs"]
mod service;

use anyhow::Result;
use neomax_core::orchestration::commands::Launcher;

use crate::context::RuntimeContext;

pub(super) fn execute(launcher: Launcher, args: &[String], context: &RuntimeContext) -> Result<()> {
    service::execute(launcher, args, context)
}

#[cfg(test)]
pub(crate) use options::AuthOptions;

#[cfg(test)]
#[path = "auth/tests.rs"]
mod tests;
