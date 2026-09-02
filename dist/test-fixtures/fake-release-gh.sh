#!/usr/bin/env bash
set -euo pipefail

: "${FAKE_GH_STATE:?}"
: "${FAKE_GH_LOG:?}"
: "${FAKE_GH_SHA:?}"
: "${FAKE_GH_TAG:?}"
mkdir -p "$FAKE_GH_STATE/assets"

argument_value() {
  local wanted="$1"
  shift
  while (($#)); do
    if [[ "$1" == "$wanted" ]]; then
      printf '%s\n' "$2"
      return
    fi
    shift
  done
  return 1
}

if [[ "$1" == api ]]; then
  printf '{"object":{"type":"commit","sha":"%s"}}\n' "$FAKE_GH_SHA"
  exit 0
fi

[[ "$1" == release ]] || exit 2
operation="$2"
shift 2
case "$operation" in
  view)
    [[ -f "$FAKE_GH_STATE/exists" ]] || exit 1
    json="$(argument_value --json "$@" 2>/dev/null || true)"
    case "$json" in
      '') exit 0 ;;
      tagName) printf '%s\n' "$FAKE_GH_TAG" ;;
      name) cat "$FAKE_GH_STATE/name" ;;
      isDraft) cat "$FAKE_GH_STATE/draft" ;;
      isPrerelease) cat "$FAKE_GH_STATE/prerelease" ;;
      url) printf '%s\n' 'https://example.invalid/release/v0.1.0' ;;
      assets)
        find "$FAKE_GH_STATE/assets" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | sort
        ;;
      *) exit 2 ;;
    esac
    ;;
  create)
    touch "$FAKE_GH_STATE/exists"
    argument_value --title "$@" > "$FAKE_GH_STATE/name"
    printf '%s\n' true > "$FAKE_GH_STATE/draft"
    printf '%s\n' false > "$FAKE_GH_STATE/prerelease"
    printf '%s\n' create >> "$FAKE_GH_LOG"
    ;;
  edit)
    title="$(argument_value --title "$@" 2>/dev/null || true)"
    [[ -z "$title" ]] || printf '%s\n' "$title" > "$FAKE_GH_STATE/name"
    for argument in "$@"; do
      case "$argument" in
        --draft=true) printf '%s\n' true > "$FAKE_GH_STATE/draft" ;;
        --draft=false) printf '%s\n' false > "$FAKE_GH_STATE/draft" ;;
        --prerelease=false) printf '%s\n' false > "$FAKE_GH_STATE/prerelease" ;;
      esac
    done
    printf 'edit draft=%s\n' "$(cat "$FAKE_GH_STATE/draft")" >> "$FAKE_GH_LOG"
    ;;
  delete-asset)
    asset="$2"
    rm -f "$FAKE_GH_STATE/assets/$asset"
    printf 'delete %s\n' "$asset" >> "$FAKE_GH_LOG"
    ;;
  upload)
    shift
    uploaded=0
    while (($#)); do
      case "$1" in
        --repo) shift 2 ;;
        --*) shift ;;
        *) cp "$1" "$FAKE_GH_STATE/assets/"; uploaded=$((uploaded + 1)); shift ;;
      esac
    done
    printf 'upload %s\n' "$uploaded" >> "$FAKE_GH_LOG"
    ;;
  download)
    destination="$(argument_value --dir "$@")"
    mkdir -p "$destination"
    cp "$FAKE_GH_STATE/assets/"* "$destination/"
    printf '%s\n' download >> "$FAKE_GH_LOG"
    ;;
  *) exit 2 ;;
esac
