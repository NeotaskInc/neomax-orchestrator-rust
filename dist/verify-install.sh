#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

archive=""
version=""
target=""
while (($#)); do
  case "$1" in
    --archive)
      (($# >= 2)) || die '--archive requires a file'
      archive="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --target)
      (($# >= 2)) || die '--target requires a value'
      target="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: verify-install.sh --archive FILE --version VERSION --target TARGET'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$archive" && -f "$archive" ]] || die '--archive is required'
[[ -n "$version" ]] || die '--version is required'
[[ -n "$target" ]] || die '--target is required'
validate_version "$version"
validate_target "$target"
archive_digest="$(sha256_file "$archive")"
"$SCRIPT_DIR/check-package.sh" --archive "$archive" --version "$version" --target "$target" >/dev/null
[[ "$(sha256_file "$archive")" == "$archive_digest" ]] ||
  die 'release archive changed after safety validation'

temporary="$(mktemp -d "${TMPDIR:-/tmp}/neomax-install-check.XXXXXX")"
trap 'rm -rf "$temporary"' EXIT
root_name="$(archive_root "$version" "$target")"
case "$archive" in
  *.tar.gz)
    tar -xzf "$archive" -C "$temporary"
    ;;
  *.zip)
    if command -v unzip >/dev/null 2>&1; then
      unzip -q "$archive" -d "$temporary"
    elif zip_tool="$(seven_zip 2>/dev/null || true)"; [[ -n "$zip_tool" ]]; then
      "$zip_tool" x -y -o"$temporary" "$archive" >/dev/null
    else
      die 'zip installation verification requires unzip or 7z'
    fi
    ;;
  *)
    die "unsupported archive format: $archive"
    ;;
esac

root="$temporary/$root_name"
"$SCRIPT_DIR/check-package.sh" --directory "$root" --version "$version" --target "$target" >/dev/null
bin="$root/bin"
fakebin="$temporary/fake-bin"
mkdir -p "$fakebin"
invocations="$temporary/provider-invocations.log"
touch "$invocations"

for provider in claude codex opencode kimi grok; do
  if is_windows_target "$target"; then
    printf '%s\n' '@echo off' '>>"%NEOMAX_VERIFY_INVOCATIONS%" echo provider' > "$fakebin/$provider.cmd"
  else
    printf '%s\n' '#!/bin/sh' 'printf "%s\\n" "$0" >> "$NEOMAX_VERIFY_INVOCATIONS"' > "$fakebin/$provider"
    chmod 0755 "$fakebin/$provider"
  fi
done

export HOME="$temporary/home"
export USERPROFILE="$HOME"
export XDG_CONFIG_HOME="$HOME/.config"
export NEOMAX_HOME="$HOME/.neomax"
export NEOMAX_DRY_RUN=1
export NEOMAX_TEST_NO_NETWORK=1
export NEOMAX_VERIFY_INVOCATIONS="$invocations"
export HTTP_PROXY='http://127.0.0.1:9'
export HTTPS_PROXY='http://127.0.0.1:9'
export ALL_PROXY='http://127.0.0.1:9'
export NO_PROXY=''
mkdir -p "$HOME"
export PATH="$fakebin:$bin:$PATH"

run_surface() {
  local name="$1"
  local executable
  executable="$bin/$(binary_name "$target" "$name")"
  [[ -x "$executable" ]] || die "installed executable is not runnable: $name"
  "$executable" --version >/dev/null
  "$executable" --help >/dev/null
}

assert_help_contains() {
  local name="$1"
  local needle="$2"
  local executable output
  executable="$bin/$(binary_name "$target" "$name")"
  output="$("$executable" --help 2>&1)" || die "$name --help failed"
  [[ -n "$output" ]] || die "$name --help produced no output"
  printf '%s\n' "$output" | grep -Fq "$needle" ||
    die "$name --help is missing: $needle"
}

for alias in "${ALIASES[@]}"; do
  run_surface "$alias"
done
for auxiliary in "${AUXILIARIES[@]}"; do
  run_surface "$auxiliary"
done

for launcher in neomax neomax-cli cmax cdxmax ocmax kmax gmax; do
  assert_help_contains "$launcher" 'portal'
  assert_help_contains "$launcher" 'rotate'
  assert_help_contains "$launcher" 'usage'
done
assert_help_contains neomax 'Universal auxiliary executables:'
assert_help_contains neomax-portal 'neomax-portal'
assert_help_contains neomax-usage-agent 'Collect local Neomax provider usage'
assert_help_contains neomax-worktrees 'coordinated Git worktree sets'

main="$bin/$(binary_name "$target" neomax)"
"$main" config show >/dev/null
"$main" config set max-subagents 3 >/dev/null
"$main" config show >/dev/null

install_root="$temporary/installed"
export NEOMAX_INSTALL_ROOT="$install_root"
"$main" install --no-usage-agent --package-root "$root" >/dev/null
installed_bin="$install_root/bin"
for alias in "${ALIASES[@]}"; do
  installed="$installed_bin/$(binary_name "$target" "$alias")"
  [[ -x "$installed" ]] || die "native installer did not install $alias"
  "$installed" --version >/dev/null
done
for auxiliary in "${AUXILIARIES[@]}"; do
  installed="$installed_bin/$(binary_name "$target" "$auxiliary")"
  [[ -x "$installed" ]] || die "native installer did not install $auxiliary"
  "$installed" --version >/dev/null
done
for workflow_path in \
  "$HOME/.claude/commands/neomax.md" \
  "$HOME/.claude/commands/project.md" \
  "$HOME/.codex/prompts/neomax.md" \
  "$HOME/.codex/prompts/project.md" \
  "$HOME/.config/opencode/commands/neomax.md" \
  "$HOME/.config/opencode/commands/project.md" \
  "$HOME/.kimi-code/skills/neomax/SKILL.md" \
  "$HOME/.kimi-code/skills/project/SKILL.md" \
  "$HOME/.kimi-code/agents/neomax.md" \
  "$HOME/.grok/commands/neomax.md" \
  "$HOME/.grok/commands/project.md"; do
  [[ -f "$workflow_path" ]] || die "native installer did not install workflow: $workflow_path"
done
for project_workflow_path in \
  "$HOME/.claude/commands/project.md" \
  "$HOME/.codex/prompts/project.md" \
  "$HOME/.config/opencode/commands/project.md" \
  "$HOME/.kimi-code/skills/project/SKILL.md" \
  "$HOME/.grok/commands/project.md"; do
  grep -Fq 'neomax projects --json' "$project_workflow_path" ||
    die "installed project workflow is missing the canonical project command: $project_workflow_path"
done
for shell_asset in "${SHELL_ASSETS[@]}"; do
  [[ -f "$install_root/share/neomax/shell/$shell_asset" ]] ||
    die "native installer did not install shell asset: $shell_asset"
done
grep -Fq 'usage-hook' "$HOME/.claude/settings.json" || die 'native installer did not install Claude usage hook'
grep -Fq 'turn-hook' "$HOME/.claude/settings.json" || die 'native installer did not install Claude turn hook'
"$installed_bin/$(binary_name "$target" neomax)" uninstall >/dev/null
[[ ! -e "$install_root/share/neomax/install-manifest.json" ]] || die 'native uninstall left its manifest'
[[ ! -e "$HOME/.claude/commands/neomax.md" ]] || die 'native uninstall left Claude workflow'
[[ ! -e "$HOME/.claude/commands/project.md" ]] || die 'native uninstall left Claude project workflow'
[[ ! -e "$HOME/.codex/prompts/project.md" ]] || die 'native uninstall left Codex project workflow'
[[ ! -e "$HOME/.config/opencode/commands/project.md" ]] || die 'native uninstall left OpenCode project workflow'
[[ ! -e "$HOME/.kimi-code/skills/neomax/SKILL.md" ]] || die 'native uninstall left Kimi workflow'
[[ ! -e "$HOME/.kimi-code/skills/project/SKILL.md" ]] || die 'native uninstall left Kimi project workflow'
[[ ! -e "$HOME/.kimi-code/agents/neomax.md" ]] || die 'native uninstall left Kimi agent'
[[ ! -e "$HOME/.grok/commands/project.md" ]] || die 'native uninstall left Grok project workflow'
for shell_asset in "${SHELL_ASSETS[@]}"; do
  [[ ! -e "$install_root/share/neomax/shell/$shell_asset" ]] ||
    die "native uninstall left shell asset: $shell_asset"
done

[[ ! -s "$invocations" ]] || die 'provider executable was invoked during installation verification'
printf '%s\n' "$archive"
