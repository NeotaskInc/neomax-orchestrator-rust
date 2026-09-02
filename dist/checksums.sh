#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

output=""
files=()
while (($#)); do
  case "$1" in
    --output)
      (($# >= 2)) || die '--output requires a path'
      output="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: checksums.sh [--output SHA256SUMS] ARCHIVE...'
      exit 0
      ;;
    --)
      shift
      files+=("$@")
      break
      ;;
    *)
      files+=("$1")
      shift
      ;;
  esac
done

((${#files[@]} > 0)) || die 'at least one archive is required'
if [[ -z "$output" ]]; then
  output="$(dirname "${files[0]}")/SHA256SUMS"
fi
mkdir -p "$(dirname "$output")"
tmp="${output}.tmp.$$"
trap 'rm -f "$tmp"' EXIT

for file in "${files[@]}"; do
  [[ -f "$file" ]] || die "archive not found: $file"
  printf '%s  %s\n' "$(sha256_file "$file")" "$(basename "$file")" >> "$tmp"
done
mv "$tmp" "$output"
trap - EXIT
printf '%s\n' "$output"
