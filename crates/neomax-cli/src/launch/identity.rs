use neomax_core::Engine;
use neomax_core::orchestration::commands::Launcher;

pub(crate) fn invocation_name(launcher: Launcher) -> &'static str {
    match launcher {
        Launcher::Universal => "neomax",
        Launcher::ProviderOrchestrator(Engine::Claude) => "cmax",
        Launcher::ProviderOrchestrator(Engine::Codex) => "cdxmax",
        Launcher::ProviderOrchestrator(Engine::Opencode) => "ocmax",
        Launcher::ProviderOrchestrator(Engine::Kimi) => "kmax",
        Launcher::ProviderOrchestrator(Engine::Grok) => "gmax",
        Launcher::AccountHelper(Engine::Codex) => "cdx",
        Launcher::AccountHelper(Engine::Opencode) => "ocx",
        Launcher::AccountHelper(Engine::Kimi) => "kmx",
        Launcher::AccountHelper(Engine::Grok) => "gmx",
        Launcher::AccountHelper(Engine::Claude) => "claude-helper",
    }
}
