#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
asset="$root/assets/shell/neomax-aliases.zsh"

command -v zsh >/dev/null 2>&1 || {
  printf '%s\n' 'zsh is not installed; skipping dynamic shortcut test'
  exit 0
}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/neomax-shell-shortcuts.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
home="$tmp/home"
bin="$tmp/bin"
log="$tmp/invocations.log"
mkdir -p \
  "$home/.claude-acct2" \
  "$home/.codex-acct10" \
  "$home/.opencode-acct3" \
  "$home/.kimi-code-acct4" \
  "$home/.grok-acct5" \
  "$bin"

for launcher in cmax cdx ocx kmx gmx; do
  launcher_path="$bin/$launcher"
  printf '%s\n' \
    '#!/bin/sh' \
    'printf "%s:" "${0##*/}" >> "$NEOMAX_SHORTCUT_LOG"' \
    'for argument do printf "<%s>" "$argument" >> "$NEOMAX_SHORTCUT_LOG"; done' \
    'printf "\\n" >> "$NEOMAX_SHORTCUT_LOG"' \
    >"$launcher_path"
  chmod 755 "$launcher_path"
done

HOME="$home" PATH="$bin:/usr/bin:/bin" NEOMAX_SHORTCUT_LOG="$log" \
  zsh -f -c '
    source "$1"
    claude1 "space value"
    claude2 "line one" "line two"
    codex1 codex
    codex10 codex-ten
    opencode3 opencode
    kimi4 kimi
    grok5 grok
    if (( $+functions[claude3] )); then exit 10; fi
    if (( $+functions[codex2] )); then exit 11; fi
    true
  ' zsh "$asset"

expected="$tmp/expected.log"
printf '%s\n' \
  'cmax:<1><space value>' \
  'cmax:<2><line one><line two>' \
  'cdx:<run><1><codex>' \
  'cdx:<run><10><codex-ten>' \
  'ocx:<run><3><opencode>' \
  'kmx:<run><4><kimi>' \
  'gmx:<run><5><grok>' \
  >"$expected"
cmp -s "$expected" "$log" || {
  printf '%s\n' 'dynamic account shortcut output differs from the canonical helper contract' >&2
  diff -u "$expected" "$log" >&2 || true
  exit 1
}

profile="$tmp/zshrc"
printf '%s\n' '# user-owned profile content' >"$profile"
cp "$profile" "$tmp/zshrc.original"
"$root/scripts/neomax-shell-shortcuts.sh" install \
  --profile "$profile" --asset "$asset" >/dev/null
grep -F '# >>> neomax account shortcuts >>>' "$profile" >/dev/null 2>&1 || {
  printf '%s\n' 'profile installation did not add its ownership marker' >&2
  exit 1
}
"$root/scripts/neomax-shell-shortcuts.sh" uninstall --profile "$profile" >/dev/null
cmp -s "$tmp/zshrc.original" "$profile" || {
  printf '%s\n' 'profile uninstall did not restore unrelated profile content' >&2
  diff -u "$tmp/zshrc.original" "$profile" >&2 || true
  exit 1
}

printf '%s\n' 'dynamic shell shortcut checks passed'
