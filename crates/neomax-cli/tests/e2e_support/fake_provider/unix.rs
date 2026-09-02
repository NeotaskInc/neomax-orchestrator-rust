use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use neomax_core::Engine;

use super::common;

pub(super) fn fake_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "fake-claude",
        Engine::Codex => "fake-codex",
        Engine::Opencode => "fake-opencode",
        Engine::Kimi => "fake-kimi",
        Engine::Grok => "fake-grok",
    }
}

pub(super) fn write_fake_provider(
    bin_dir: &Path,
    engine: Engine,
    provider: &str,
) -> io::Result<()> {
    let path = bin_dir.join(fake_name(engine));
    write_provider(&path, provider)?;
    write_provider(&bin_dir.join(provider), provider)
}

fn write_provider(path: &Path, provider: &str) -> io::Result<()> {
    let profile_env = common::profile_env(provider);
    let secret_lines = common::SECRET_ENV_NAMES
        .iter()
        .map(|name| {
            let field = name.to_ascii_lowercase();
            format!("  printf 'secret_{field}=%s\\n' \"@@{{{name}:+present}}\"\n")
        })
        .collect::<String>();
    let script = format!(
        r#"#!/bin/sh
set -eu
log="@@{{NEOMAX_E2E_LOG:-/dev/null}}"
profile="@@{{{profile_env}-}}"
{{
{secret_lines}  printf 'provider={provider}\n'
  printf 'program=%s\n' "$0"
  printf 'profile=%s\n' "$profile"
  printf 'neomax_bin=%s\n' "@@{{NEOMAX_BIN-}}"
  printf 'tool_manifest=%s\n' "@@{{NEOMAX_TOOL_MANIFEST-}}"
  printf 'tool_instruction=%s\n' "@@{{NEOMAX_TOOL_INSTRUCTION-}}"
  printf 'tool_depth=%s\n' "@@{{NEOMAX_TOOL_DEPTH-}}"
  printf 'tool_max_depth=%s\n' "@@{{NEOMAX_TOOL_MAX_DEPTH-}}"
  printf 'tool_policy=%s\n' "@@{{NEOMAX_TOOL_POLICY-}}"
  printf 'max_subagents=%s\n' "@@{{NEOMAX_MAX_SUBAGENTS-}}"
  printf 'role=%s\n' "@@{{NEOMAX_ROLE-}}"
  printf 'mode=%s\n' "@@{{NEOMAX_MODE-}}"
  printf 'worker=%s\n' "@@{{NEOMAX_WORKER-}}"
  printf 'network_proxy=%s\n' "@@{{ALL_PROXY-}}"
  printf 'stdin_probe='
  if IFS= read -r line; then printf '%s\n' "$line"; else printf '<eof>\n'; fi
  printf 'args='
  for arg do printf '%s\037' "$arg"; done
  printf '\n__NEOMAX_E2E_RECORD_END__\n'
}} >> "$log"
if [ "@@{{NEOMAX_E2E_BEHAVIOR-}}" = "sleep" ]; then
  exec /bin/sleep 60
fi
if [ "@@{{NEOMAX_E2E_BEHAVIOR-}}" = "rotate" ] && printf '%s' "$profile" | grep -q 'acct1' && [ -f "$profile/.claude.json" ] && grep -q 'fixture-.claude-acct1' "$profile/.claude.json"; then
  exec /bin/sleep 60
fi
for arg do
  case "$arg" in
    --version|login|logout|models|status|orchestrator|orch) exit 0 ;;
  esac
done
if [ "$#" -eq 0 ]; then
  exit 0
fi
case "{provider}" in
  claude) printf '%s\n' '{{"type":"result","subtype":"success","session_id":"session-claude","result":"fixture"}}' ;;
  codex) thread_number=$(grep -c '^provider=codex' "$log" 2>/dev/null || true); printf '%s\n' '{{"type":"thread.started","thread_id":"session-codex-'"$thread_number"'"}}'; printf '%s\n' '{{"type":"turn.completed"}}' ;;
  opencode) printf '%s\n' '{{"type":"text","sessionID":"session-opencode","part":{{"text":"fixture"}}}}'; printf '%s\n' '{{"type":"step_finish","part":{{"reason":"stop"}}}}' ;;
  kimi) printf '%s\n' '{{"role":"assistant","content":"fixture"}}'; printf '%s\n' '{{"role":"meta","type":"session.resume_hint","session_id":"session-kimi"}}' ;;
  grok) printf '%s\n' '{{"type":"text","data":"fixture"}}'; printf '%s\n' '{{"type":"end","stopReason":"end_turn","sessionId":"session-grok"}}' ;;
esac
"#,
    )
    .replace("@@", "$");
    fs::write(path, script)?;
    set_executable(path)
}

pub(super) fn write_fake_security(bin_dir: &Path) -> io::Result<()> {
    let path = bin_dir.join("security");
    fs::write(&path, "#!/bin/sh\nexit 1\n")?;
    set_executable(&path)
}

pub(super) fn write_poison_provider(bin_dir: &Path, provider: &str) -> io::Result<()> {
    let path = bin_dir.join(provider);
    fs::write(
        &path,
        "#!/bin/sh\nset -eu\nprintf 'poison-provider-invoked=%s\\n' \"$0\" >> \"${NEOMAX_E2E_POISON_LOG:-/dev/null}\"\nexit 125\n",
    )?;
    set_executable(&path)
}

fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

pub(super) fn alias_path(bin_dir: &Path, alias: &str) -> PathBuf {
    bin_dir.join(alias)
}

pub(super) fn create_alias(path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_neomax"), path)
}
