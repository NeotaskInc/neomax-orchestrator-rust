#!/bin/sh
set -eu

begin_marker='# >>> neomax account shortcuts >>>'
end_marker='# <<< neomax account shortcuts <<<'

usage() {
  printf '%s\n' \
    'usage: neomax-shell-shortcuts.sh install --profile FILE [--asset FILE]' \
    '       neomax-shell-shortcuts.sh uninstall --profile FILE'
}

fail() {
  printf 'neomax shell shortcuts: %s\n' "$1" >&2
  exit 2
}

profile=''
asset=''
operation=''
while [ "$#" -gt 0 ]; do
  case "$1" in
    install|uninstall)
      [ -z "$operation" ] || fail 'operation was specified more than once'
      operation="$1"
      shift
      ;;
    --profile)
      [ $# -ge 2 ] || fail '--profile requires a file'
      profile=$2
      shift 2
      ;;
    --asset)
      [ $# -ge 2 ] || fail '--asset requires a file'
      asset=$2
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage >&2
      fail "unknown option: $1"
      ;;
  esac
done

[ -n "$operation" ] || { usage >&2; exit 2; }
[ -n "$profile" ] || fail '--profile is required'

case "$profile" in
  */*) ;;
  *) fail '--profile must be a path' ;;
esac

if [ "$operation" = install ]; then
  [ -n "$asset" ] || fail '--asset is required for install'
  [ -f "$asset" ] || fail "shortcut asset does not exist: $asset"
  case "$asset" in
    /*) ;;
    *) fail '--asset must be an absolute path' ;;
  esac
fi

if [ -L "$profile" ]; then
  fail "profile must not be a symlink: $profile"
fi
profile_dir=${profile%/*}
[ "$profile_dir" != "$profile" ] || profile_dir='.'
if [ -e "$profile" ] && [ ! -f "$profile" ]; then
  fail "profile is not a regular file: $profile"
fi
if [ ! -e "$profile" ]; then
  if [ "$operation" = install ]; then
    [ -d "$profile_dir" ] || fail "profile parent does not exist: $profile_dir"
    : >"$profile"
  else
    printf 'no Neomax account shortcuts found in %s\n' "$profile"
    exit 0
  fi
fi

tmp=$(mktemp "$profile_dir/.neomax-shell-profile.XXXXXX")
trap 'rm -f "$tmp"' EXIT HUP INT TERM

if ! awk -v begin="$begin_marker" -v end="$end_marker" '
  BEGIN { inside = 0; blocks = 0; malformed = 0 }
  $0 == begin {
    if (inside || blocks) {
      malformed = 1
    } else {
      inside = 1
      blocks = 1
    }
    next
  }
  $0 == end {
    if (!inside) {
      malformed = 1
    } else {
      inside = 0
    }
    next
  }
  !inside { print }
  END {
    if (inside) malformed = 1
    if (malformed) exit 1
  }
' "$profile" >"$tmp"; then
  fail "profile contains a malformed Neomax account shortcut block: $profile"
fi

if [ "$operation" = install ]; then
  quoted_asset=$(printf '%s' "$asset" | sed "s/'/'\\''/g; s/^/'/; s/$/'/")
  {
    printf '%s\n' "$begin_marker"
    printf 'if [ -r %s ]; then\n' "$quoted_asset"
    printf '  source %s\n' "$quoted_asset"
    printf 'fi\n%s\n' "$end_marker"
  } >>"$tmp"
fi

mode=$(stat -c '%a' "$profile" 2>/dev/null || stat -f '%Lp' "$profile")
mv "$tmp" "$profile"
trap - EXIT HUP INT TERM
chmod "$mode" "$profile"

if [ "$operation" = install ]; then
  printf 'installed Neomax account shortcuts in %s\n' "$profile"
else
  printf 'removed Neomax account shortcuts from %s\n' "$profile"
fi
