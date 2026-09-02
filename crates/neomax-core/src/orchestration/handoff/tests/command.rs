use std::collections::BTreeMap;

use super::super::{
    LaunchOptions, PreservedEnvironment, ShellKind, build_launch_plan, default_kickoff,
    launcher_for, render_shell_command_for, shell_quote,
};
use crate::Engine;

fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("NEOMAX_FLEET".into(), "claude,codex,opencode".into()),
        ("NEOMAX_CLAUDE_MODEL".into(), "claude-fable-5[1m]".into()),
        ("NEOMAX_CODEX_MODEL".into(), "gpt-5.6-terra".into()),
        ("NEOMAX_OPENCODE_MODEL".into(), "opencode/big-pickle".into()),
        ("NEOMAX_KIMI_MODEL".into(), "kimi-code/k3".into()),
        ("NEOMAX_GROK_MODEL".into(), "grok-4.6".into()),
        (
            crate::agent_tools::NEOMAX_TOOL_POLICY_ENV.into(),
            "full".into(),
        ),
        (
            crate::agent_tools::NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV.into(),
            "1".into(),
        ),
        ("NEOMAX_ROLE".into(), "opencode".into()),
        ("SSH_TTY".into(), "/dev/ttys001".into()),
        ("CLAUDE_CONFIG_DIR".into(), "/profiles/.claude-acct2".into()),
        ("CODEX_HOME".into(), "/profiles/.codex-acct2".into()),
        ("XDG_DATA_HOME".into(), "/profiles/.opencode-acct2".into()),
        ("KIMI_CODE_HOME".into(), "/profiles/.kimi-code-acct2".into()),
        ("GROK_HOME".into(), "/profiles/.grok-acct2".into()),
        ("OPENCODE_API_KEY".into(), "must-not-be-copied".into()),
        (
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            "must-not-be-copied".into(),
        ),
        ("SECRET_TOKEN".into(), "must-not-be-copied".into()),
    ])
}

#[test]
fn builds_exact_commands_for_all_provider_launchers() {
    for engine in Engine::ALL {
        let options = LaunchOptions::from_environment(
            engine,
            "1",
            "orch",
            "manual /rotate",
            "/workspace/project",
            "resume now",
            &environment(),
        );
        let plan = build_launch_plan(&options).unwrap();
        assert_eq!(plan.launcher, launcher_for(engine));
        assert_eq!(
            plan.args.first().map(String::as_str),
            Some("--orchestrator")
        );
        if cfg!(windows) {
            assert!(
                plan.shell_command
                    .starts_with("Set-Location -LiteralPath '/workspace/project'; & ")
            );
        } else {
            assert!(
                plan.shell_command
                    .starts_with("cd '/workspace/project' && ")
            );
        }
        assert!(
            plan.shell_command
                .contains("'--workers' 'claude,codex,opencode'")
        );
        assert!(
            plan.shell_command
                .contains("'--opencode-model' 'opencode/big-pickle'")
        );
        assert!(plan.headless);
    }
}

#[test]
fn all_provider_handoffs_keep_kickoff_out_of_kimi_interactive_argv() {
    for engine in Engine::ALL {
        let options = LaunchOptions::from_environment(
            engine,
            "1",
            "2",
            "quota",
            "/workspace/project",
            "durable kickoff",
            &BTreeMap::new(),
        );
        let plan = build_launch_plan(&options).unwrap();
        if engine == Engine::Kimi {
            assert_eq!(plan.args, vec!["2"], "Kimi root handoff: {plan:?}");
        } else {
            assert_eq!(
                plan.args.last().map(String::as_str),
                Some("durable kickoff")
            );
        }
    }
}

#[test]
fn handoff_model_overrides_use_catalog_validation_and_aliases() {
    let mut options = LaunchOptions::from_environment(
        Engine::Kimi,
        "1",
        "2",
        "model policy",
        "/workspace/project",
        "continue",
        &BTreeMap::new(),
    );
    options.model_overrides = BTreeMap::from([(Engine::Kimi, "k2.7".into())]);
    let plan = build_launch_plan(&options).unwrap();
    assert!(
        plan.args
            .windows(2)
            .any(|pair| pair == ["--kimi-model", "kimi-code/kimi-for-coding"])
    );

    options
        .model_overrides
        .insert(Engine::Opencode, "big-pickle".into());
    let error = build_launch_plan(&options).unwrap_err();
    assert!(error.to_string().contains("provider/model"));
}

#[test]
fn preserves_only_safe_scope_and_model_environment() {
    let preserved = PreservedEnvironment::from_environment(&environment());
    assert_eq!(
        preserved.worker_scope().as_deref(),
        Some("claude,codex,opencode")
    );
    assert_eq!(preserved.model_overrides().len(), 5);
    assert!(!preserved.values.contains_key("SECRET_TOKEN"));
    assert!(!preserved.values.contains_key("OPENCODE_API_KEY"));
    assert!(!preserved.values.contains_key("CLAUDE_CONFIG_DIR"));
    assert!(!preserved.values.contains_key("NEOMAX_ROLE"));
    assert!(!preserved.values.contains_key("NEOMAX_ORCH_RESERVED"));
    assert_eq!(
        preserved
            .values
            .get(crate::agent_tools::NEOMAX_TOOL_POLICY_ENV),
        Some(&"full".to_string())
    );
    assert_eq!(
        preserved
            .values
            .get(crate::agent_tools::NEOMAX_ALLOW_FULL_TOOL_POLICY_ENV),
        Some(&"1".to_string())
    );
}

#[test]
fn account_numbers_and_kickoff_are_rendered_without_loss() {
    let options = LaunchOptions {
        engine: Engine::Claude,
        source_account: "2".into(),
        target_account: "3".into(),
        reason: "manual /rotate".into(),
        cwd: "/tmp/space/project".into(),
        kickoff: "line one\nline two; $(touch /tmp/nope)".into(),
        worker_scope: None,
        model_overrides: BTreeMap::new(),
        environment: PreservedEnvironment::default(),
        headless: true,
        session_id: None,
        resume: false,
    };
    let plan = build_launch_plan(&options).unwrap();
    assert_eq!(plan.args[0], "3");
    assert_eq!(plan.args[1], "line one line two; $(touch /tmp/nope)");
    assert!(
        plan.shell_command
            .contains("'line one line two; $(touch /tmp/nope)'")
    );
}

#[test]
fn kimi_handoff_resumes_a_durable_session_without_a_positional_task() {
    let mut options = LaunchOptions::from_environment(
        Engine::Kimi,
        "1",
        "2",
        "quota",
        "/workspace/project",
        "resume this durable task",
        &BTreeMap::new(),
    );
    options.session_id = Some("kimi-session-42".into());
    options.resume = true;
    let plan = build_launch_plan(&options).unwrap();
    assert_eq!(
        plan.args,
        vec!["2", "--session-id", "kimi-session-42", "--resume"]
    );
    assert!(!plan.args.iter().any(|arg| arg.contains("durable task")));
}

#[test]
fn kimi_handoff_without_a_session_still_uses_interactive_root_shape() {
    let options = LaunchOptions::from_environment(
        Engine::Kimi,
        "1",
        "2",
        "quota",
        "/workspace/project",
        "resume this durable task",
        &BTreeMap::new(),
    );
    let plan = build_launch_plan(&options).unwrap();
    assert_eq!(plan.args, vec!["2"]);
}

#[test]
fn shell_quote_handles_single_quotes_and_shell_metacharacters() {
    assert_eq!(
        shell_quote("a'b; $(touch nope)"),
        "'a'\\''b; $(touch nope)' ".trim_end()
    );
    assert_eq!(shell_quote("plain"), "'plain'");
}

#[test]
fn display_renderers_quote_spaces_and_metacharacters_per_shell() {
    let cwd = std::path::Path::new("/workspace/space;project");
    let launcher = "neo max";
    let args = vec![
        "account 2".into(),
        "task; $(touch nope)".into(),
        "a'b".into(),
    ];

    assert_eq!(
        render_shell_command_for(ShellKind::Posix, cwd, launcher, &args),
        "cd '/workspace/space;project' && 'neo max' 'account 2' 'task; $(touch nope)' 'a'\\''b'"
    );
    assert_eq!(
        render_shell_command_for(ShellKind::PowerShell, cwd, launcher, &args),
        "Set-Location -LiteralPath '/workspace/space;project'; & 'neo max' 'account 2' 'task; $(touch nope)' 'a''b'"
    );

    let cmd_args = vec!["task & | < > ( ) ^ % !".into()];
    assert_eq!(
        render_shell_command_for(ShellKind::Cmd, cwd, launcher, &cmd_args),
        "cd /d \"/workspace/space;project\" && \"neo max\" \"task & | < > ( ) ^ %% !\""
    );
    let cmd_path = vec!["C:\\work space\\".into(), "a\"b".into()];
    assert_eq!(
        render_shell_command_for(ShellKind::Cmd, cwd, launcher, &cmd_path),
        "cd /d \"/workspace/space;project\" && \"neo max\" \"C:\\work space\\\\\" \"a\\\"b\""
    );
}

#[test]
fn host_shell_kind_is_explicit_and_stable() {
    if cfg!(windows) {
        assert_eq!(ShellKind::host(), ShellKind::PowerShell);
    } else {
        assert_eq!(ShellKind::host(), ShellKind::Posix);
    }
}

#[test]
fn dry_run_plan_does_not_invoke_any_platform() {
    let options = LaunchOptions::from_environment(
        Engine::Claude,
        "1",
        "2",
        "dry-run",
        "/workspace",
        default_kickoff(Engine::Claude, "1"),
        &BTreeMap::new(),
    );
    let plan = build_launch_plan(&options).unwrap();
    assert!(!plan.headless);
    if cfg!(windows) {
        assert!(plan.shell_command.contains("& 'cmax' '2'"));
    } else {
        assert!(plan.shell_command.contains("cmax '2'"));
    }
}
