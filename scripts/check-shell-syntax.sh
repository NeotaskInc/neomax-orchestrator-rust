#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
status=0

while IFS= read -r script; do
  if ! bash -n "$script"; then
    status=1
  fi
done <<EOF
$root/install.sh
$(find "$root/dist" "$root/scripts" -type f -name '*.sh' -print | sort)
EOF

if command -v zsh >/dev/null 2>&1; then
  zsh -n "$root/assets/shell/neomax-aliases.zsh"
fi
sh -n "$root/assets/shell/neomax-shell-shortcuts.sh"

exit "$status"
