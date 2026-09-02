#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

input=""
version=""
target=""
while (($#)); do
  case "$1" in
    --archive|--directory)
      (($# >= 2)) || die "$1 requires a path"
      input="$2"
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
    --help|-h)
      printf '%s\n' 'usage: check-package.sh --archive FILE|--directory DIR --version VERSION --target TARGET'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$input" ]] || die '--archive or --directory is required'
[[ -n "$version" ]] || die '--version is required'
[[ -n "$target" ]] || die '--target is required'
validate_version "$version"
validate_target "$target"
root_name="$(archive_root "$version" "$target")"

temporary=""
cleanup() {
  [[ -z "$temporary" ]] || rm -rf "$temporary"
}
trap cleanup EXIT

manifest_paths() {
  sed -nE 's/^[[:space:]]*\{"path":"([^"]+)".*/\1/p' "$1"
}

expected_manifest_paths() {
  local alias auxiliary
  for alias in "${ALIASES[@]}"; do
    printf 'bin/%s\n' "$(binary_name "$target" "$alias")"
  done
  for auxiliary in "${AUXILIARIES[@]}"; do
    printf 'bin/%s\n' "$(binary_name "$target" "$auxiliary")"
  done
  printf '%s\n' \
    'share/neomax/opencode-model-policy.json' \
    'LICENSE' \
    'README.md' \
    'docs/INSTALLATION.md'
  for shell_asset in "${SHELL_ASSETS[@]}"; do
    printf 'share/neomax/shell/%s\n' "$shell_asset"
  done
  for workflow in "${WORKFLOW_ASSETS[@]}"; do
    printf 'share/neomax/workflows/%s.md\n' "$workflow"
  done
  printf '%s\n' 'share/neomax/agents/neomax-kimi.md'
}

validate_manifest_paths() {
  local manifest="$1"
  local expected actual path
  expected="$(expected_manifest_paths | sort)"
  actual="$(manifest_paths "$manifest" | sort)"
  [[ "$actual" == "$expected" ]] || die 'release manifest command surface is not exact'
  while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    [[ "$path" != *'"'* && "$path" != *'\\'* ]] || die "invalid release manifest path: $path"
  done < <(manifest_paths "$manifest")
}

archive_path_is_expected() {
  local entry="${1%/}"
  local expected expected_paths
  [[ "$entry" == "$root_name" ]] && return 0
  expected_paths="$({
    expected_manifest_paths
    printf '%s\n' 'RELEASE-MANIFEST.json'
  })"
  while IFS= read -r expected; do
    [[ "$entry" == "$root_name/$expected" ]] && return 0
    [[ "$root_name/$expected" == "$entry/"* ]] && return 0
  done <<< "$expected_paths"
  return 1
}

reject_archive_entry() {
  local entry="${1%/}"
  [[ -n "$entry" ]] || die 'archive contains an empty entry name'
  [[ "$entry" != /* && "$entry" != *'\\'* ]] || die "unsafe archive entry: $entry"
  local component
  local old_ifs="$IFS"
  IFS=/
  read -ra components <<< "$entry"
  IFS="$old_ifs"
  for component in "${components[@]}"; do
    [[ -n "$component" && "$component" != '.' && "$component" != '..' ]] ||
      die "unsafe archive entry: $entry"
  done
  [[ "$entry" == "$root_name" || "$entry" == "$root_name/"* ]] ||
    die "archive entry is outside package root: $entry"
  archive_path_is_expected "$entry" || die "unmanifested archive entry: $entry"
}

validate_archive_names() {
  local entries="$1"
  local normalized duplicate entry
  normalized="$(printf '%s\n' "$entries" | sed 's:/*$::' | sort)"
  duplicate="$(printf '%s\n' "$normalized" | uniq -d | sed -n '1p')"
  [[ -z "$duplicate" ]] || die "archive contains a duplicate entry: $duplicate"
  while IFS= read -r entry; do
    [[ -n "$entry" ]] || continue
    reject_archive_entry "$entry"
  done <<< "$entries"
}

validate_tar_archive() {
  local archive="$1"
  require_command tar
  local entries listing kind link_target
  entries="$(tar -tzf "$archive")" || die "could not list archive: $archive"
  validate_archive_names "$entries"
  while IFS= read -r listing; do
    [[ -n "$listing" ]] || continue
    kind="${listing:0:1}"
    case "$kind" in
      -|d) ;;
      l)
        link_target="${listing##* -> }"
        [[ "$link_target" == 'neomax' ]] ||
          die "unsafe archive symlink target: $link_target"
        ;;
      *) die "unsupported archive entry type in: $listing" ;;
    esac
  done < <(tar -tvzf "$archive")
}

zip_listing() {
  if command -v zipinfo >/dev/null 2>&1; then
    zipinfo -l "$1"
  elif command -v unzip >/dev/null 2>&1; then
    unzip -Z -l "$1"
  else
    die 'zip verification requires zipinfo or unzip'
  fi
}

validate_zip_archive() {
  local archive="$1"
  require_command unzip
  local entries listing kind
  entries="$(unzip -Z1 "$archive")" || die "could not list archive: $archive"
  validate_archive_names "$entries"
  while IFS= read -r listing; do
    [[ -n "$listing" ]] || continue
    kind="${listing:0:1}"
    case "$kind" in
      -|d|\?) ;;
      *) die "unsupported ZIP archive entry type in: $listing" ;;
    esac
  done < <(
    zip_listing "$archive" |
      awk 'substr($0, 1, 10) ~ /^[dl?-][rwxstST-]{9}$/ { print }'
  )
}

validate_extracted_entries() {
  local root="$1"
  local manifest="$2"
  local absolute relative link target
  local unsafe
  unsafe="$(find "$root" -mindepth 1 ! \( -type f -o -type l -o -type d \) -print -quit)"
  [[ -z "$unsafe" ]] || die "unsupported extracted entry type: ${unsafe#"$root/"}"
  while IFS= read -r absolute; do
    relative="${absolute#"$root/"}"
    [[ -n "$relative" ]] || continue
    [[ "$relative" == RELEASE-MANIFEST.json ]] && continue
    grep -Fxq "$relative" <(manifest_paths "$manifest") ||
      die "release archive contains an unmanifested entry: $relative"
  done < <(find "$root" -mindepth 1 \( -type f -o -type l \) -print | sort)
  while IFS= read -r link; do
    target="$(readlink "$link")"
    [[ "$target" == 'neomax' ]] || die "unsafe package symlink: ${link#"$root/"} -> $target"
  done < <(find "$root" -mindepth 1 -type l -print)
}

if [[ -d "$input" ]]; then
  if [[ "$(basename "$input")" == "$root_name" ]]; then
    root="$input"
  elif [[ -d "$input/$root_name" ]]; then
    root="$input/$root_name"
  else
    die "package directory does not contain $root_name"
  fi
else
  [[ -f "$input" ]] || die "package archive not found: $input"
  temporary="$(mktemp -d "${TMPDIR:-/tmp}/neomax-check.XXXXXX")"
  case "$input" in
    *.tar.gz)
      validate_tar_archive "$input"
      tar -xzf "$input" -C "$temporary"
      ;;
    *.zip)
      validate_zip_archive "$input"
      unzip -q "$input" -d "$temporary"
      ;;
    *)
      die "unsupported archive format: $input"
      ;;
  esac
  root="$temporary/$root_name"
fi

[[ -d "$root" ]] || die "missing package root: $root"
manifest="$root/RELEASE-MANIFEST.json"
[[ -f "$manifest" ]] || die 'missing RELEASE-MANIFEST.json'
[[ ! -L "$manifest" ]] || die 'RELEASE-MANIFEST.json must be a regular file'
validate_manifest_paths "$manifest"
validate_extracted_entries "$root" "$manifest"
grep -Fq "\"product\":\"$PRODUCT\"" "$manifest" || die 'manifest product mismatch'
grep -Fq "\"version\":\"$version\"" "$manifest" || die 'manifest version mismatch'
grep -Fq "\"target\":\"$target\"" "$manifest" || die 'manifest target mismatch'

check_file() {
  local relative="$1"
  local path="$root/$relative"
  [[ -f "$path" ]] || die "missing package file: $relative"
  local line
  line="$(grep -F "\"path\":\"$relative\"" "$manifest" || true)"
  [[ "$line" == *'"kind":"file"'* ]] || die "manifest does not describe file: $relative"
  local expected
  expected="$(printf '%s\n' "$line" | sed -nE 's/.*"sha256":"([0-9a-f]{64})".*/\1/p')"
  [[ -n "$expected" ]] || die "manifest hash missing: $relative"
  [[ "$(sha256_file "$path")" == "$expected" ]] || die "manifest hash mismatch: $relative"
}

check_alias() {
  local alias="$1"
  local name
  name="$(binary_name "$target" "$alias")"
  local path="$root/bin/$name"
  [[ -e "$path" ]] || die "missing alias: $name"
  [[ "$alias" == neomax ]] && return 0
  if is_windows_target "$target"; then
    [[ ! -L "$path" ]] || die "Windows alias must be a copy: $name"
    cmp -s "$root/bin/$(binary_name "$target" neomax)" "$path" || die "Windows alias differs from neomax: $name"
  else
    [[ -L "$path" ]] || die "Unix alias must be a symlink: $name"
    [[ "$(readlink "$path")" == neomax ]] || die "Unix alias target mismatch: $name"
  fi
}

for alias in "${ALIASES[@]}"; do
  check_alias "$alias"
done
for auxiliary in "${AUXILIARIES[@]}"; do
  check_file "bin/$(binary_name "$target" "$auxiliary")"
done
check_file "bin/$(binary_name "$target" neomax)"
check_file share/neomax/opencode-model-policy.json
check_file LICENSE
check_file README.md
check_file docs/INSTALLATION.md
for shell_asset in "${SHELL_ASSETS[@]}"; do
  check_file "share/neomax/shell/$shell_asset"
done
for workflow in "${WORKFLOW_ASSETS[@]}"; do
  check_file "share/neomax/workflows/$workflow.md"
done
check_file share/neomax/agents/neomax-kimi.md
kimi_agent="$root/share/neomax/agents/neomax-kimi.md"
[[ "$(sed -n '1p' "$kimi_agent")" == '---' ]] || die 'Kimi agent asset is missing Markdown frontmatter'
awk 'NR > 1 && $0 == "---" {found = 1} END {exit found ? 0 : 1}' "$kimi_agent" || die 'Kimi agent asset frontmatter is not closed'
grep -Fq 'name: neomax' "$kimi_agent" || die 'Kimi agent asset name mismatch'
grep -Fq 'description:' "$kimi_agent" || die 'Kimi agent asset description is missing'
grep -Fq '${base_prompt}' "$kimi_agent" || die 'Kimi agent asset must preserve the built-in base prompt'
! grep -Fq 'kimi_cli.tools' "$kimi_agent" || die 'Kimi agent asset contains obsolete tool identifiers'
! grep -Fq 'multiagent:Task' "$kimi_agent" || die 'Kimi agent asset contains obsolete tool identifiers'

if ! is_windows_target "$target"; then
  for alias in "${ALIASES[@]}"; do
    [[ -x "$root/bin/$(binary_name "$target" "$alias")" ]] || die "Unix alias is not executable: $alias"
  done
  for auxiliary in "${AUXILIARIES[@]}"; do
    [[ -x "$root/bin/$(binary_name "$target" "$auxiliary")" ]] || die "Unix auxiliary is not executable: $auxiliary"
  done
fi

printf '%s\n' "$input"
