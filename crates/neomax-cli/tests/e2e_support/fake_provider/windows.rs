use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use neomax_core::Engine;

const FIXTURE_SOURCE: &str = r####"
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, BufRead, Write};
use std::thread;
use std::time::Duration;

const SECRET_ENV_NAMES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "OPENCODE_API_KEY",
    "OPENCODE_ZEN_API_KEY",
    "KIMI_API_KEY",
    "KIMI_MODEL_API_KEY",
    "XAI_API_KEY",
    "GROK_API_KEY",
    "GROK_DEPLOYMENT_KEY",
    "GOOGLE_API_KEY",
    "VERTEXAI_API_KEY",
];

fn main() {
    if run().is_err() {
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let log = env::var_os("NEOMAX_E2E_LOG").unwrap_or_default();
    if log.is_empty() {
        return Ok(());
    }

    let provider = provider_name();
    let profile = env::var_os(profile_env(&provider))
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stdin_probe = read_stdin_probe();
    let args = env::args_os()
        .skip(1)
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    let mut fields = vec![
        format!("provider={provider}"),
        format!("program={}", env::current_exe()?.display()),
        format!("profile={profile}"),
        format!("neomax_bin={}", env_value("NEOMAX_BIN")),
        format!("tool_manifest={}", env_value("NEOMAX_TOOL_MANIFEST")),
        format!("tool_instruction={}", env_value("NEOMAX_TOOL_INSTRUCTION")),
        format!("tool_depth={}", env_value("NEOMAX_TOOL_DEPTH")),
        format!("tool_max_depth={}", env_value("NEOMAX_TOOL_MAX_DEPTH")),
        format!("tool_policy={}", env_value("NEOMAX_TOOL_POLICY")),
        format!("max_subagents={}", env_value("NEOMAX_MAX_SUBAGENTS")),
        format!("role={}", env_value("NEOMAX_ROLE")),
        format!("mode={}", env_value("NEOMAX_MODE")),
        format!("worker={}", env_value("NEOMAX_WORKER")),
        format!("network_proxy={}", env_value("ALL_PROXY")),
    ];
    for name in SECRET_ENV_NAMES {
        let present = env::var_os(name).is_some_and(|value| !value.is_empty());
        fields.push(format!(
            "secret_{}={}",
            name.to_ascii_lowercase(),
            if present { "present" } else { "" }
        ));
    }
    fields.push(format!("stdin_probe={stdin_probe}"));
    fields.push(format!(
        "args64={}:{}",
        args.len(),
        args.iter()
            .map(|value| base64(value.as_bytes()))
            .collect::<Vec<_>>()
            .join("\x1f")
    ));
    fields.push("__NEOMAX_E2E_RECORD_END__".into());

    let line_end = if cfg!(windows) { "\r\n" } else { "\n" };
    let mut record = fields.join(line_end);
    record.push_str(line_end);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?
        .write_all(record.as_bytes())?;

    let behavior = env_value("NEOMAX_E2E_BEHAVIOR");
    if behavior.eq_ignore_ascii_case("sleep") {
        thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    if behavior.eq_ignore_ascii_case("rotate")
        && provider == "claude"
        && profile.contains("acct1")
        && fs::read_to_string(std::path::Path::new(&profile).join(".claude.json"))
            .is_ok_and(|identity| identity.contains("fixture-.claude-acct1"))
    {
        thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    if args.iter().any(|argument| {
        matches!(
            argument.as_str(),
            "login" | "logout" | "--version" | "models" | "status" | "orchestrator" | "orch"
        )
    }) || args.is_empty()
    {
        return Ok(());
    }

    match provider.as_str() {
        "claude" => println!(
            r#"{{"type":"result","subtype":"success","session_id":"session-claude","result":"fixture"}}"#
        ),
        "codex" => {
            let thread_number = fs::read_to_string(log)
                .unwrap_or_default()
                .lines()
                .filter(|line| line.trim_end_matches('\r') == "provider=codex")
                .count();
            println!(r#"{{"type":"thread.started","thread_id":"session-codex-{thread_number}"}}"#);
            println!(r#"{{"type":"turn.completed"}}"#);
        }
        "opencode" => {
            println!(r#"{{"type":"text","sessionID":"session-opencode","part":{{"text":"fixture"}}}}"#);
            println!(r#"{{"type":"step_finish","part":{{"reason":"stop"}}}}"#);
        }
        "kimi" => {
            println!(r#"{{"role":"assistant","content":"fixture"}}"#);
            println!(r#"{{"role":"meta","type":"session.resume_hint","session_id":"session-kimi"}}"#);
        }
        "grok" => {
            println!(r#"{{"type":"text","data":"fixture"}}"#);
            println!(r#"{{"type":"end","stopReason":"end_turn","sessionId":"session-grok"}}"#);
        }
        _ => {}
    }
    Ok(())
}

fn provider_name() -> String {
    let stem = env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .unwrap_or_default();
    stem.strip_prefix("fake-").unwrap_or(&stem).to_ascii_lowercase()
}

fn profile_env(provider: &str) -> &'static str {
    match provider {
        "claude" => "CLAUDE_CONFIG_DIR",
        "codex" => "CODEX_HOME",
        "opencode" => "XDG_DATA_HOME",
        "kimi" => "KIMI_CODE_HOME",
        "grok" => "GROK_HOME",
        _ => "",
    }
}

fn env_value(name: &str) -> String {
    env::var(name).unwrap_or_default()
}

fn read_stdin_probe() -> String {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return "<eof>".into();
    }
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => "<eof>".into(),
        Ok(_) => line.trim_end_matches(['\r', '\n']).into(),
        Err(_) => "<eof>".into(),
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[((first & 0x03) << 4 | chunk.get(1).copied().unwrap_or(0) >> 4) as usize] as char);
        if let Some(second) = chunk.get(1) {
            encoded.push(TABLE[((second & 0x0f) << 2 | chunk.get(2).copied().unwrap_or(0) >> 6) as usize] as char);
        } else {
            encoded.push('=');
        }
        if let Some(third) = chunk.get(2) {
            encoded.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
    }
    encoded
}
"####;

pub(super) fn fake_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "fake-claude.exe",
        Engine::Codex => "fake-codex.exe",
        Engine::Opencode => "fake-opencode.exe",
        Engine::Kimi => "fake-kimi.exe",
        Engine::Grok => "fake-grok.exe",
    }
}

pub(super) fn write_fake_provider(
    bin_dir: &Path,
    engine: Engine,
    provider: &str,
) -> io::Result<()> {
    let path = bin_dir.join(fake_name(engine));
    write_provider(&path)?;
    write_provider(&bin_dir.join(format!("{provider}.exe")))
}

fn write_provider(path: &Path) -> io::Result<()> {
    fs::write(path, compiled_provider()?)
}

fn compiled_provider() -> io::Result<&'static [u8]> {
    static COMPILED: OnceLock<Result<Vec<u8>, String>> = OnceLock::new();
    match COMPILED.get_or_init(|| compile_provider().map_err(|error| error.to_string())) {
        Ok(bytes) => Ok(bytes.as_slice()),
        Err(error) => Err(io::Error::other(error.clone())),
    }
}

fn compile_provider() -> io::Result<Vec<u8>> {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("fake-provider.rs");
    let output = temp.path().join("fake-provider.exe");
    fs::write(&source, FIXTURE_SOURCE)?;
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let status = Command::new(rustc)
        .args(["--edition=2021", "-O"])
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "rustc failed to compile the Windows fake provider: {status}"
        )));
    }
    fs::read(output)
}

pub(super) fn write_fake_security(bin_dir: &Path) -> io::Result<()> {
    fs::write(bin_dir.join("security.cmd"), "@echo off\r\nexit /b 1\r\n")
}

pub(super) fn write_poison_provider(bin_dir: &Path, provider: &str) -> io::Result<()> {
    let path = bin_dir.join(format!("{provider}.cmd"));
    fs::write(
        path,
        "@echo off\r\nsetlocal\r\n>>\"%NEOMAX_E2E_POISON_LOG%\" echo(poison-provider-invoked=%~f0\r\nexit /b 125\r\n",
    )
}

pub(super) fn alias_path(bin_dir: &Path, alias: &str) -> PathBuf {
    bin_dir.join(format!("{alias}.exe"))
}

pub(super) fn create_alias(path: &Path) -> io::Result<()> {
    fs::copy(env!("CARGO_BIN_EXE_neomax"), path).map(|_| ())
}
