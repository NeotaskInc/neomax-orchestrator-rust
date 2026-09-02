#[path = "e2e_support/mod.rs"]
mod support;

use neomax_core::Engine;

use support::{
    E2eHarness,
    assertions::{
        assert_not_worker_tagged, assert_root_argv, assert_worker_argv, assert_worker_tools,
    },
};

#[test]
fn provider_model_overrides_are_passed_to_every_provider_cli() {
    for (engine, launcher, flag, model) in [
        (
            Engine::Claude,
            "cmax",
            "--claude-model",
            "fixture/claude-model",
        ),
        (
            Engine::Codex,
            "cdxmax",
            "--codex-model",
            "fixture/codex-model",
        ),
        (
            Engine::Opencode,
            "ocmax",
            "--opencode-model",
            "fixture/opencode-model",
        ),
        (Engine::Kimi, "kmax", "--kimi-model", "fixture/kimi-model"),
        (Engine::Grok, "gmax", "--grok-model", "fixture/grok-model"),
    ] {
        let harness = E2eHarness::new([engine]);
        let mut args = vec![
            "--json".to_owned(),
            "--foreground".to_owned(),
            flag.to_owned(),
            model.to_owned(),
        ];
        if engine != Engine::Kimi {
            args.push("model fixture".into());
        }
        let result = harness.run_alias(launcher, args);
        let report = result.json();
        assert_eq!(report["model"], model, "launcher {launcher}");
        let invocations = harness.invocations();
        assert_eq!(invocations.len(), 1, "launcher {launcher}");
        assert_eq!(invocations[0].model_arg(), Some(model));
        assert_root_argv(&invocations[0], engine, launcher);
        assert_not_worker_tagged(&invocations[0], launcher);
        harness.assert_hermetic_invocations();
    }
}

#[test]
fn every_worker_receives_the_complete_tool_contract_and_shared_subagent_cap() {
    for engine in Engine::ALL {
        let harness = E2eHarness::new([engine]);
        let result = harness.run_with_env(
            [
                "dispatch",
                "--json",
                "--foreground",
                "--brief",
                "--engine",
                engine.as_str(),
                "tool fixture",
            ],
            harness.authorized_orchestrator_environment(),
        );
        let report = result.json();
        assert_eq!(report["status"], "done", "engine {engine}: {report}");
        let invocation = harness.invocations().pop().expect("fake invocation");

        let manifest = invocation.field("tool_manifest").expect("tool manifest");
        assert!(std::path::Path::new(manifest).is_file(), "{engine}");
        let manifest_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
        assert!(manifest_json["commands"].is_array(), "{engine}");
        assert!(
            invocation
                .field("neomax_bin")
                .is_some_and(|path| std::path::Path::new(path).is_absolute())
        );
        assert!(
            invocation
                .field("tool_instruction")
                .is_some_and(|value| value.contains("NEOMAX_BIN"))
        );
        assert!(
            invocation
                .field("tool_depth")
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            invocation
                .field("tool_max_depth")
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(invocation.field("tool_policy"), Some("worker"));
        assert_eq!(invocation.field("max_subagents"), Some("17"));
        assert_eq!(invocation.field("role"), Some(engine.as_str()));
        assert_worker_argv(&invocation, engine, &format!("worker {engine}"));
        assert_worker_tools(&invocation, &format!("worker {engine}"));
        assert_eq!(invocation.field("worker"), Some("1"));
        harness.assert_hermetic_invocations();
    }
}

#[cfg(windows)]
#[test]
fn windows_native_provider_argv_preserves_shell_sensitive_model_arguments() {
    let model = r#"fixture/%PATH%/!bang!/"quoted"/&pipe|caret^less<greater>"#;
    for (engine, launcher, model_flag, trailing_flag, task) in [
        (
            Engine::Claude,
            "cmax",
            "--claude-model",
            "--dangerously-skip-permissions",
            Some("transport fixture"),
        ),
        (
            Engine::Codex,
            "cdxmax",
            "--codex-model",
            "-a",
            Some("transport fixture"),
        ),
        (
            Engine::Opencode,
            "ocmax",
            "--opencode-model",
            "--agent",
            Some("transport fixture"),
        ),
        (Engine::Kimi, "kmax", "--kimi-model", "--auto", None),
        (
            Engine::Grok,
            "gmax",
            "--grok-model",
            "--always-approve",
            Some("transport fixture"),
        ),
    ] {
        let harness = E2eHarness::new([engine]);
        let mut args = vec![
            "--json".to_owned(),
            "--foreground".to_owned(),
            model_flag.to_owned(),
            model.to_owned(),
        ];
        if let Some(task) = task {
            args.push(task.to_owned());
        }

        let result = harness.run_alias(launcher, args);
        assert_eq!(result.json()["model"], model, "launcher {launcher}");
        let invocation = harness.invocations().pop().expect("fake invocation");
        assert_eq!(invocation.model_arg(), Some(model), "launcher {launcher}");
        assert!(
            invocation.has_arg(trailing_flag),
            "launcher {launcher} lost the provider flag after its model"
        );
        harness.assert_hermetic_invocations();
    }
}
