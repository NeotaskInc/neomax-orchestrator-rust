#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
exec "$root/assets/shell/neomax-shell-shortcuts.sh" "$@"
