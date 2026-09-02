#[path = "e2e_install_support/mod.rs"]
mod support;

use support::{ALIASES, AUXILIARIES, InstallFixture, SHELL_ASSETS, WORKFLOWS};

#[test]
fn install_and_uninstall_are_complete_and_hermetic() {
    let fixture = InstallFixture::new();
    fixture.materialize_package();

    let install = fixture.run(&fixture.paths_args("install"));
    assert!(
        install.status.success(),
        "install failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&install.stdout),
        String::from_utf8_lossy(&install.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&install.stdout).expect("install JSON");
    assert_eq!(report["product"], "neomax");
    assert_eq!(report["upgraded"], false);

    for alias in ALIASES {
        let path = fixture
            .destination
            .join("bin")
            .join(support::binary_name(alias));
        assert!(path.exists(), "installed alias missing: {}", path.display());
        let version = fixture.command(&path).args(["--version"]).output().unwrap();
        assert!(
            version.status.success(),
            "installed alias failed: {}\n{}",
            alias,
            String::from_utf8_lossy(&version.stderr)
        );
        assert!(
            String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")),
            "installed alias did not report its version: {}",
            String::from_utf8_lossy(&version.stdout)
        );
    }
    for auxiliary in AUXILIARIES {
        assert!(
            fixture
                .destination
                .join("bin")
                .join(support::binary_name(auxiliary))
                .is_file(),
            "auxiliary missing: {auxiliary}"
        );
    }
    for asset in [
        "LICENSE",
        "README.md",
        "opencode-model-policy.json",
        "INSTALLATION.md",
    ] {
        assert!(
            fixture
                .destination
                .join("share/neomax")
                .join(asset)
                .is_file(),
            "installed asset missing: {asset}"
        );
    }
    for workflow in WORKFLOWS {
        assert!(
            fixture
                .destination
                .join("share/neomax/workflows")
                .join(workflow)
                .is_file(),
            "installed workflow missing: {workflow}"
        );
    }
    for asset in SHELL_ASSETS {
        assert!(
            fixture
                .destination
                .join("share/neomax/shell")
                .join(asset)
                .is_file()
        );
    }
    assert!(
        fixture
            .destination
            .join("share/neomax/agents/neomax-kimi.md")
            .is_file()
    );
    assert!(fixture.home.join(".claude/commands/neomax.md").is_file());
    assert!(fixture.home.join(".claude/commands/project.md").is_file());
    assert!(fixture.home.join(".codex/prompts/neomax.md").is_file());
    assert!(fixture.home.join(".codex/prompts/project.md").is_file());
    assert!(
        fixture
            .home
            .join(".config/opencode/commands/neomax.md")
            .is_file()
    );
    assert!(
        fixture
            .home
            .join(".config/opencode/commands/project.md")
            .is_file()
    );
    assert!(
        fixture
            .home
            .join(".kimi-code/skills/neomax/SKILL.md")
            .is_file()
    );
    assert!(
        fixture
            .home
            .join(".kimi-code/skills/project/SKILL.md")
            .is_file()
    );
    assert!(fixture.home.join(".kimi-code/agents/neomax.md").is_file());
    assert!(fixture.home.join(".grok/commands/neomax.md").is_file());
    assert!(fixture.home.join(".grok/commands/project.md").is_file());
    for project_workflow in [
        fixture.home.join(".claude/commands/project.md"),
        fixture.home.join(".codex/prompts/project.md"),
        fixture.home.join(".config/opencode/commands/project.md"),
        fixture.home.join(".kimi-code/skills/project/SKILL.md"),
        fixture.home.join(".grok/commands/project.md"),
    ] {
        let content = std::fs::read_to_string(&project_workflow).unwrap();
        assert!(
            content.contains("neomax projects --json"),
            "project workflow lacks canonical project listing: {}",
            project_workflow.display()
        );
        assert!(
            content.contains("neomax orient --json"),
            "project workflow lacks canonical context refresh: {}",
            project_workflow.display()
        );
        assert!(
            content.contains("Provider entry:"),
            "project workflow was not rendered for its provider: {}",
            project_workflow.display()
        );
    }
    assert!(fixture.home.join(".claude/settings.json").is_file());
    assert!(
        !fixture.provider_log.exists(),
        "installation unexpectedly started a provider"
    );

    let uninstall = fixture.run(&fixture.paths_args("uninstall"));
    assert!(
        uninstall.status.success(),
        "uninstall failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&uninstall.stdout),
        String::from_utf8_lossy(&uninstall.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&uninstall.stdout).expect("uninstall JSON");
    assert_eq!(report["product"], "neomax");
    assert!(
        !fixture
            .destination
            .join("share/neomax/install-manifest.json")
            .exists()
    );
    for asset in SHELL_ASSETS {
        assert!(
            !fixture
                .destination
                .join("share/neomax/shell")
                .join(asset)
                .exists()
        );
    }
    assert!(!fixture.home.join(".claude/commands/neomax.md").exists());
    assert!(!fixture.provider_log.exists());
}
