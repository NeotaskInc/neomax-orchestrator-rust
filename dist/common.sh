#!/usr/bin/env bash
set -euo pipefail

DIST_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$DIST_ROOT/.." && pwd)"
PRODUCT="neomax"

ALIASES=(neomax neomax-cli cmax cdx cdxmax ocx ocmax kmx kmax gmx gmax)
AUXILIARIES=(neomax-portal neomax-usage-agent neomax-worktrees)
WORKFLOW_ASSETS=(neomax rotate find-issues fix-issues project)
SHELL_ASSETS=(neomax-aliases.zsh neomax-shell-shortcuts.sh)
RELEASE_TARGETS=(
  x86_64-unknown-linux-gnu
  x86_64-apple-darwin
  aarch64-apple-darwin
  x86_64-pc-windows-msvc
  aarch64-unknown-linux-gnu
  x86_64-unknown-linux-musl
  aarch64-unknown-linux-musl
)

die() {
  printf 'dist: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

root_version() {
  sed -nE 's/^version = "([^"]+)"$/\1/p' "$REPO_ROOT/Cargo.toml" | head -n 1
}

validate_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || die "invalid version: $1"
}

validate_target() {
  case "$1" in
    x86_64-apple-darwin|aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl|x86_64-pc-windows-msvc) ;;
    *) die "unsupported release target: $1" ;;
  esac
}

is_windows_target() {
  [[ "$1" == *-windows-* ]]
}

archive_extension() {
  if is_windows_target "$1"; then
    printf '%s\n' zip
  else
    printf '%s\n' tar.gz
  fi
}

archive_root() {
  printf '%s-v%s-%s\n' "$PRODUCT" "$1" "$2"
}

archive_name() {
  printf '%s-v%s-%s.%s\n' "$PRODUCT" "$1" "$2" "$(archive_extension "$2")"
}

binary_name() {
  if is_windows_target "$1"; then
    printf '%s.exe\n' "$2"
  else
    printf '%s\n' "$2"
  fi
}

normalize_sha256_output() {
  local output="$1" hash
  hash="${output%%[[:space:]]*}"
  case "$hash" in
    \\*) hash="${hash#?}" ;;
  esac
  [[ "$hash" =~ ^[0-9A-Fa-f]{64}$ ]] || die 'invalid checksum tool output'
  printf '%s\n' "$hash" | tr '[:upper:]' '[:lower:]'
}

sha256_file() {
  local output
  if command -v shasum >/dev/null 2>&1; then
    output="$(shasum -a 256 "$1")"
  elif command -v sha256sum >/dev/null 2>&1; then
    output="$(sha256sum "$1")"
  else
    die 'neither shasum nor sha256sum is available'
  fi
  normalize_sha256_output "$output"
}

file_size() {
  wc -c < "$1" | tr -d '[:space:]'
}

seven_zip() {
  if command -v 7z >/dev/null 2>&1; then
    command -v 7z
  elif [[ -x '/c/Program Files/7-Zip/7z.exe' ]]; then
    printf '%s\n' '/c/Program Files/7-Zip/7z.exe'
  else
    return 1
  fi
}

copy_binary() {
  local source="$1"
  local destination="$2"
  [[ -f "$source" ]] || die "missing build output: $source"
  mkdir -p "$(dirname "$destination")"
  cp "$source" "$destination"
  if [[ "$(uname -s 2>/dev/null || true)" != MINGW* && "$(uname -s 2>/dev/null || true)" != MSYS* ]]; then
    chmod 0755 "$destination"
  fi
}

copy_text_lf() {
  local source="$1"
  local destination="$2"
  [[ -f "$source" ]] || die "missing text asset: $source"
  mkdir -p "$(dirname "$destination")"
  sed 's/\r$//' "$source" > "$destination"
}
