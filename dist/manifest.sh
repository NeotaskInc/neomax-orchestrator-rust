#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

root=""
version=""
target=""
output=""
while (($#)); do
  case "$1" in
    --root)
      (($# >= 2)) || die '--root requires a directory'
      root="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --target)
      (($# >= 2)) || die '--target requires a value'
      target="$2"
      shift 2
      ;;
    --output)
      (($# >= 2)) || die '--output requires a path'
      output="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: manifest.sh --root PACKAGE_ROOT --version VERSION --target TARGET [--output FILE]'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$root" && -d "$root" ]] || die "package root is missing: $root"
[[ -n "$version" ]] || die 'version is required'
[[ -n "$target" ]] || die 'target is required'
validate_version "$version"
validate_target "$target"
[[ "$(basename "$root")" == "$(archive_root "$version" "$target")" ]] || die "package root name must be $(archive_root "$version" "$target")"

if [[ -z "$output" ]]; then
  output="$root/RELEASE-MANIFEST.json"
fi
mkdir -p "$(dirname "$output")"
tmp="${output}.tmp.$$"
trap 'rm -f "$tmp"' EXIT

layout="copy"
is_windows_target "$target" || layout="symlink"
printf '{\n' > "$tmp"
printf '  "schema_version":1,\n' >> "$tmp"
printf '  "product":"%s",\n' "$PRODUCT" >> "$tmp"
printf '  "version":"%s",\n' "$version" >> "$tmp"
printf '  "target":"%s",\n' "$target" >> "$tmp"
printf '  "alias_layout":"%s",\n' "$layout" >> "$tmp"
printf '  "files":[\n' >> "$tmp"

paths=()
while IFS= read -r path; do
  paths+=("$path")
done < <(find "$root" \( -type f -o -type l \) ! -name RELEASE-MANIFEST.json ! -name 'RELEASE-MANIFEST.json.tmp.*' -print | sort)

((${#paths[@]} > 0)) || die 'package contains no files'
for index in "${!paths[@]}"; do
  path="${paths[$index]}"
  relative="${path#"$root"/}"
  comma=,
  ((index == ${#paths[@]} - 1)) && comma=
  if [[ -L "$path" ]]; then
    link_target="$(readlink "$path")"
    printf '    {"path":"%s","kind":"symlink","target":"%s"}%s\n' "$relative" "$link_target" "$comma" >> "$tmp"
  else
    hash="$(sha256_file "$path")"
    size="$(file_size "$path")"
    printf '    {"path":"%s","kind":"file","size":%s,"sha256":"%s"}%s\n' "$relative" "$size" "$hash" "$comma" >> "$tmp"
  fi
done

printf '  ]\n}\n' >> "$tmp"
mv "$tmp" "$output"
trap - EXIT
printf '%s\n' "$output"
