mod auth;
mod live;
mod render;
mod session;
mod solo;
mod tick;
mod universal;

use anyhow::Result;
use neomax_core::orchestration::commands::{Command, Launcher};

use crate::context::RuntimeContext;

pub(crate) use solo::arm_profile;

pub(crate) fn execute(
    launcher: Launcher,
    command: Command,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    match command {
        Command::Rotate => universal::execute(launcher, args, context),
        Command::RotateTick => tick::execute(launcher, args, context),
        Command::SessionRotate => session::execute(launcher, args, context),
        Command::SoloRotate => solo::execute(launcher, args, context),
        Command::RotateAuth => auth::execute(launcher, args, context),
        _ => unreachable!("rotation facade received {command:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn facade_routes_each_rotation_command_without_provider_execution() {
        let fixture = fixture();
        for (command, args) in [
            (Command::Rotate, vec!["--json".into()]),
            (Command::RotateTick, vec!["--json".into()]),
            (Command::SessionRotate, vec!["--json".into()]),
            (Command::SoloRotate, vec!["--json".into()]),
            (Command::RotateAuth, vec!["--json".into()]),
        ] {
            execute(Launcher::Universal, command, &args, &fixture.context).unwrap();
        }
    }
}
