#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

command -v rg >/dev/null 2>&1 || fail 'ripgrep (rg) is required for the product-surface gate.'

if find . -path './.git' -prune -o -path './target' -prune -o -type f -print \
  | grep -E '/(ox-smoke|[^/]*smoke[^/]*)$' >/dev/null; then
  fail 'Published smoke executables and scripts are not part of the product surface.'
fi

if [ -e crates/neomax-orchestrator-rust ]; then
  fail 'A duplicated repository path exists under crates/.'
fi

matches=$(rg -n \
  "cdelegate|cmax-orchestrator|cmax-worktrees|cmax-usage-agent|bin/neotask|Command::new\\(\"neotask\"\\)|name[[:space:]]*=[[:space:]]*\"neotask\"|\"neotask\"[[:space:]]*:|NeoMax|Neo-Max" \
  --hidden --glob '!target/**' --glob '!.git/**' --glob '!scripts/check-product-surface.sh' --glob '!scripts/check-privacy-surface.sh' . || true)
if [ -n "$matches" ]; then
  fail 'Legacy or reserved executable names remain in the product surface.'
fi

# The old launchd service label is retained only as an uninstall migration
# seam. Keep the exception narrow so it cannot become a new install surface.
legacy_usagewatch_files=$(rg -l 'io\.cmax\.usagewatch' \
  --hidden --glob '!target/**' --glob '!.git/**' \
  --glob '!scripts/check-product-surface.sh' --glob '!scripts/check-privacy-surface.sh' . || true)
for file in $legacy_usagewatch_files; do
  case "$file" in
    ./crates/neomax-usage-agent/src/config.rs|./crates/neomax-usage-agent/src/install/launchd.rs|./crates/neomax-usage-agent/src/install/launchd/tests.rs) ;;
    *) fail 'The legacy usage-watch service label is allowed only in its uninstall migration seam.' ;;
  esac
done

matches=$(rg -l -i \
  '/Users/[^/[:space:]]+/' \
  --hidden --glob '!target/**' --glob '!.git/**' --glob '!scripts/check-product-surface.sh' --glob '!scripts/check-privacy-surface.sh' . || true)
if [ -n "$matches" ]; then
  fail 'Machine-specific macOS home paths are not allowed in the product surface.'
fi

home_files=$(rg -l -i '/home/[^/[:space:]]+/' \
  --hidden --glob '!target/**' --glob '!.git/**' --glob '!scripts/check-product-surface.sh' --glob '!scripts/check-privacy-surface.sh' . || true)
for file in $home_files; do
  if rg -n -i '/home/[^/[:space:]]+/' "$file" | grep -qvE '/home/(user|tester|example|test)/'; then
    fail 'Machine-specific Linux home paths are not allowed in the product surface.'
  fi
done

matches=$(rg -l -i \
  'ghp_[[:alnum:]]{20,}|github_pat_[[:alnum:]_]{20,}|sk-[[:alnum:]]{20,}|Bearer[[:space:]]+[[:alnum:]._-]{20,}|BEGIN[[:space:]]+([A-Z]+[[:space:]]+)?PRIVATE[[:space:]]+KEY' \
  --hidden --glob '!target/**' --glob '!.git/**' --glob '!scripts/check-product-surface.sh' --glob '!scripts/check-privacy-surface.sh' . || true)
if [ -n "$matches" ]; then
  fail 'Credential or private-key material is not allowed in the product surface.'
fi

matches=$(git ls-files --cached --others --exclude-standard | rg -i \
  '(^|/)(\.env(\..*)?|.*\.(pem|p12|pfx|key)|id_(rsa|ed25519)|credentials\.json|secrets\.json)$' || true)
if [ -n "$matches" ]; then
  fail 'Credential-bearing files must remain ignored and outside the product surface.'
fi
