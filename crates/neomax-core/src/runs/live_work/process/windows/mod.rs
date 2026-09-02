mod handles;
mod inspector;
mod parsing;
mod remote;
mod security;

#[cfg(test)]
pub(crate) use inspector::WindowsProcessInfo;
pub(crate) use inspector::{NativeWindowsProcessInspector, WindowsProcessInspector};
pub(crate) use parsing::{is_claude_process, profile_environment_value};
