use std::fs;

use super::super::install::install;
use super::super::types::InstallOptions;
use super::super::workflows::ensure_profile_workflows_at;
use super::support::fixture;

#[test]
fn on_demand_profiles_receive_workflows_and_claude_hooks() {
    let (package, destination, paths) = fixture();
    let home = destination.path().join("home");
    install(InstallOptions {
        package_root: Some(package.path().into()),
        paths: Some(paths.clone()),
        profile_home: Some(home.clone()),
        force: false,
    })
    .unwrap();
    let kimi_profile = home.join(".kimi-code-acct2");
    fs::create_dir_all(&kimi_profile).unwrap();
    ensure_profile_workflows_at(crate::Engine::Kimi, &kimi_profile, &home, &paths).unwrap();
    assert!(kimi_profile.join("skills/neomax/SKILL.md").is_file());
    assert!(kimi_profile.join("agents/neomax.md").is_file());
    assert!(!kimi_profile.join("SYSTEM.md").exists());
    let claude_profile = home.join(".claude-acct2");
    fs::create_dir_all(&claude_profile).unwrap();
    ensure_profile_workflows_at(crate::Engine::Claude, &claude_profile, &home, &paths).unwrap();
    assert!(claude_profile.join("commands/project.md").is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(claude_profile.join("settings.json")).unwrap()).unwrap();
    assert!(settings["hooks"]["Stop"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["hooks"].as_array().into_iter().flatten())
        .any(|hook| hook["command"].as_str().unwrap().contains("usage-hook")));

    let codex_profile = home.join(".codex-acct2");
    fs::create_dir_all(&codex_profile).unwrap();
    ensure_profile_workflows_at(crate::Engine::Codex, &codex_profile, &home, &paths).unwrap();
    assert!(codex_profile.join("prompts/project.md").is_file());

    let opencode_profile = home.join(".opencode-acct2");
    fs::create_dir_all(&opencode_profile).unwrap();
    ensure_profile_workflows_at(crate::Engine::Opencode, &opencode_profile, &home, &paths).unwrap();
    assert!(home.join(".config/opencode/commands/project.md").is_file());

    let grok_profile = home.join(".grok-acct2");
    fs::create_dir_all(&grok_profile).unwrap();
    ensure_profile_workflows_at(crate::Engine::Grok, &grok_profile, &home, &paths).unwrap();
    assert!(grok_profile.join("commands/project.md").is_file());
}
