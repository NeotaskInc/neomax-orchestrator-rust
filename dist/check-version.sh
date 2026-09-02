#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

usage() {
  printf '%s\n' 'usage: check-version.sh [--version VERSION] [--tag vVERSION] [--print]'
}

requested_version=""
tag=""
print_only=0
while (($#)); do
  case "$1" in
    --version)
      (($# >= 2)) || die '--version requires a value'
      requested_version="$2"
      shift 2
      ;;
    --tag)
      (($# >= 2)) || die '--tag requires a value'
      tag="$2"
      shift 2
      ;;
    --print)
      print_only=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

manifest_version="$(root_version)"
[[ -n "$manifest_version" ]] || die 'workspace version is missing from Cargo.toml'
validate_version "$manifest_version"

if [[ -n "$requested_version" ]]; then
  validate_version "$requested_version"
  [[ "$requested_version" == "$manifest_version" ]] || die "requested version $requested_version does not match workspace version $manifest_version"
fi

if [[ -n "$tag" ]]; then
  [[ "$tag" == v* ]] || die "release tag must start with v: $tag"
  tag_version="${tag#v}"
  validate_version "$tag_version"
  [[ "$tag_version" == "$manifest_version" ]] || die "tag $tag does not match workspace version $manifest_version"
fi

if ((print_only)); then
  printf '%s\n' "$manifest_version"
else
  printf 'version=%s\n' "$manifest_version"
fi
