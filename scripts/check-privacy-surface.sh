#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

command -v rg >/dev/null 2>&1 || fail 'ripgrep (rg) is required for the privacy gate.'

local_pattern_file=${NEOMAX_PRIVACY_PATTERNS_FILE:-.neomax-privacy.local}

if [ -e "$local_pattern_file" ] && git ls-files --error-unmatch -- "$local_pattern_file" >/dev/null 2>&1; then
  fail 'The local privacy pattern file must not be tracked.'
fi

private_patterns=''
if [ -f "$local_pattern_file" ]; then
  private_patterns=$(sed -e '/^[[:space:]]*#/d' -e '/^[[:space:]]*$/d' "$local_pattern_file")
fi

assert_pattern_present() {
  value=$1
  pattern=$2
  if ! printf '%s\n' "$value" | rg -qi -- "$pattern"; then
    fail 'Privacy gate self-test failed to detect a prohibited identifier.'
  fi
}

assert_pattern_absent() {
  value=$1
  pattern=$2
  if printf '%s\n' "$value" | rg -qi -- "$pattern"; then
    fail 'Privacy gate self-test rejected a sanitized fixture.'
  fi
}

run_self_tests() {
  assert_pattern_present '/Users/example/' '/Users/[^/[:space:]]+/'
  assert_pattern_present '/home/example-user/' '/home/[^/[:space:]]+/'
  assert_pattern_present 'Bearer fixture-token-value-123456' 'Bearer[[:space:]]+[[:alnum:]._-]{20,}'
  assert_pattern_present '-----BEGIN PRIVATE KEY-----' 'BEGIN[[:space:]]+([A-Z]+[[:space:]]+)?PRIVATE[[:space:]]+KEY'
  assert_pattern_absent '/tmp/user/project' '/home/[^/[:space:]]+/'
}

run_self_tests
if [ "${1:-}" = '--self-test' ]; then
  exit 0
fi

private_pattern_matches() {
  [ -n "$private_patterns" ] || return 1
  while IFS= read -r pattern; do
    if rg -l -I -i \
      -e "$pattern" \
      --hidden --glob '!target/**' --glob '!.git/**' \
      --glob '!scripts/check-privacy-surface.sh' \
      --glob '!scripts/check-product-surface.sh' . >/dev/null 2>&1; then
      return 0
    fi
  done <<EOF
$private_patterns
EOF
  return 1
}

matches=$(rg -l -I -i \
  'ghp_[[:alnum:]]{20,}|github_pat_[[:alnum:]_]{20,}|sk-[[:alnum:]]{20,}|Bearer[[:space:]]+[[:alnum:]._-]{20,}|BEGIN[[:space:]]+([A-Z]+[[:space:]]+)?PRIVATE[[:space:]]+KEY' \
  --hidden --glob '!target/**' --glob '!.git/**' \
  --glob '!scripts/check-privacy-surface.sh' \
  --glob '!scripts/check-product-surface.sh' . || true)
if [ -n "$matches" ]; then
  fail 'Credential or private-key material is not allowed in the product surface.'
fi

if private_pattern_matches; then
  fail 'A local private identifier or workspace pattern matched the product surface.'
fi

mac_paths=$(rg -n -I -i '/Users/[^/[:space:]]+/' \
  --hidden --glob '!target/**' --glob '!.git/**' \
  --glob '!scripts/check-privacy-surface.sh' \
  --glob '!scripts/check-product-surface.sh' . || true)
if [ -n "$mac_paths" ]; then
  fail 'Machine-specific macOS home paths are not allowed in the product surface.'
fi

linux_paths=$(rg -n -I -i '/home/[^/[:space:]]+/' \
  --hidden --glob '!target/**' --glob '!.git/**' \
  --glob '!scripts/check-privacy-surface.sh' \
  --glob '!scripts/check-product-surface.sh' . \
  | grep -vE '/home/(user|tester|example|test)/' || true)
if [ -n "$linux_paths" ]; then
  fail 'Machine-specific Linux home paths are not allowed in the product surface.'
fi
