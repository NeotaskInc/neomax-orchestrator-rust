use neomax_core::Engine;
use neomax_core::agent_tools::ToolManifest;
use std::fs;

use super::invocation::Invocation;

#[allow(dead_code)]
pub(crate) fn assert_root_argv(invocation: &Invocation, engine: Engine, label: &str) {
    match engine {
        Engine::Claude => assert!(!invocation.has_arg("-p"), "{label} used Claude -p"),
        Engine::Codex => assert!(!invocation.has_arg("exec"), "{label} used Codex exec"),
        Engine::Opencode => assert!(!invocation.has_arg("run"), "{label} used OpenCode run"),
        Engine::Kimi => {
            assert!(
                !invocation.has_arg("--prompt") && !invocation.has_arg("-p"),
                "{label} used Kimi headless prompt flags"
            );
        }
        Engine::Grok => assert!(
            !invocation.has_arg("--single"),
            "{label} used Grok --single"
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn assert_worker_argv(invocation: &Invocation, engine: Engine, label: &str) {
    match engine {
        Engine::Claude => assert!(invocation.has_arg("-p"), "{label} lacks Claude -p"),
        Engine::Codex => assert!(invocation.has_arg("exec"), "{label} lacks Codex exec"),
        Engine::Opencode => assert!(invocation.has_arg("run"), "{label} lacks OpenCode run"),
        Engine::Kimi => assert!(
            invocation.has_arg("--prompt"),
            "{label} lacks Kimi --prompt"
        ),
        Engine::Grok => assert!(
            invocation.has_arg("--single"),
            "{label} lacks Grok --single"
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn assert_plan_argv(invocation: &Invocation, engine: Engine, label: &str) {
    match engine {
        Engine::Claude => {
            assert_worker_argv(invocation, engine, label);
            assert_eq!(
                invocation.arg_value("--permission-mode"),
                Some("plan"),
                "{label}"
            );
            assert!(
                !invocation.has_arg("--dangerously-skip-permissions"),
                "{label}"
            );
        }
        Engine::Codex => {
            assert_worker_argv(invocation, engine, label);
            assert_eq!(invocation.arg_value("-s"), Some("read-only"), "{label}");
            assert!(
                !invocation.has_arg("--dangerously-bypass-approvals-and-sandbox"),
                "{label}"
            );
        }
        Engine::Opencode => {
            assert_worker_argv(invocation, engine, label);
            assert_eq!(invocation.arg_value("--agent"), Some("plan"), "{label}");
            assert!(!invocation.has_arg("--auto"), "{label}");
        }
        Engine::Kimi => {
            assert_worker_argv(invocation, engine, label);
            assert!(!invocation.has_arg("--auto"), "{label}");
            assert!(
                invocation
                    .field("profile")
                    .is_some_and(|profile| profile.contains("kimi-plan-")),
                "{label} did not use Kimi's temporary read-only plan home"
            );
        }
        Engine::Grok => {
            assert_worker_argv(invocation, engine, label);
            assert_eq!(
                invocation.arg_value("--permission-mode"),
                Some("plan"),
                "{label}"
            );
            assert!(!invocation.has_arg("--always-approve"), "{label}");
        }
    }
}

#[allow(dead_code)]
pub(crate) fn assert_not_worker_tagged(invocation: &Invocation, label: &str) {
    assert_eq!(
        invocation.field("worker"),
        Some(""),
        "{label} was incorrectly tagged NEOMAX_WORKER"
    );
}

#[allow(dead_code)]
pub(crate) fn assert_orchestrator_tools(invocation: &Invocation, label: &str) {
    assert_eq!(
        invocation.field("tool_policy"),
        Some("orchestrator"),
        "{label} did not receive orchestrator tool policy"
    );
    let instruction = invocation
        .field("tool_instruction")
        .unwrap_or_else(|| panic!("{label} did not receive a tool instruction"));
    assert!(
        !instruction.contains("do not start another worker"),
        "{label} received the worker-only tool instruction: {instruction}"
    );
    let manifest = invocation
        .field("tool_manifest")
        .unwrap_or_else(|| panic!("{label} did not receive a tool manifest"));
    assert!(std::path::Path::new(manifest).is_file(), "{label}");
    let manifest_text = fs::read_to_string(manifest).expect("read canonical tool manifest");
    let manifest_value: ToolManifest =
        serde_json::from_str(&manifest_text).expect("decode canonical tool manifest");
    assert_eq!(
        manifest_value,
        ToolManifest::canonical(),
        "{label} received an incomplete or non-canonical tool manifest"
    );
    assert!(
        invocation
            .field("tool_depth")
            .is_some_and(|depth| depth == "0"),
        "{label} did not receive root tool depth"
    );
    assert!(
        invocation
            .field("tool_max_depth")
            .is_some_and(|depth| !depth.is_empty()),
        "{label} did not receive a tool recursion limit"
    );
    assert!(
        invocation
            .field("neomax_bin")
            .is_some_and(|path| std::path::Path::new(path).is_absolute()),
        "{label} did not receive an absolute Neomax binary"
    );
}

#[allow(dead_code)]
pub(crate) fn assert_worker_tools(invocation: &Invocation, label: &str) {
    assert_eq!(
        invocation.field("tool_policy"),
        Some("worker"),
        "{label} did not receive worker tool policy"
    );
    let instruction = invocation
        .field("tool_instruction")
        .unwrap_or_else(|| panic!("{label} did not receive a tool instruction"));
    assert!(
        instruction.contains("do not start another worker"),
        "{label} did not receive the worker-only tool instruction: {instruction}"
    );
    let manifest = invocation
        .field("tool_manifest")
        .unwrap_or_else(|| panic!("{label} did not receive a tool manifest"));
    let manifest_text = fs::read_to_string(manifest).expect("read canonical tool manifest");
    let manifest_value: ToolManifest =
        serde_json::from_str(&manifest_text).expect("decode canonical tool manifest");
    assert_eq!(
        manifest_value,
        ToolManifest::canonical(),
        "{label} received an incomplete or non-canonical tool manifest"
    );
    assert!(
        invocation
            .field("tool_depth")
            .is_some_and(|depth| depth == "1"),
        "{label} did not receive child tool depth"
    );
    assert!(
        invocation
            .field("tool_max_depth")
            .is_some_and(|depth| !depth.is_empty()),
        "{label} did not receive a tool recursion limit"
    );
}

#[allow(dead_code)]
pub(crate) fn assert_authorization_error(error: &str, selector: &str) {
    assert!(
        error.contains("NEOMAX_ROLE")
            || error.contains("NEOMAX_WORKER")
            || error.contains("NEOMAX_ALLOW_WORKER_DISPATCH"),
        "unauthorized {selector} dispatch returned an unrelated error: {error}"
    );
}

#[allow(dead_code)]
pub(crate) fn pinned_orchestrator_alias(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "cmax",
        Engine::Codex => "cdxmax",
        Engine::Opencode => "ocmax",
        Engine::Kimi => "kmax",
        Engine::Grok => "gmax",
    }
}
