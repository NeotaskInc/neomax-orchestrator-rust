#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
source "$SCRIPT_DIR/release-assets.sh"

artifacts_dir=""
version=""
output_dir=""
repo_root="$REPO_ROOT"
while (($#)); do
  case "$1" in
    --artifacts-dir)
      (($# >= 2)) || die '--artifacts-dir requires a directory'
      artifacts_dir="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --output-dir)
      (($# >= 2)) || die '--output-dir requires a directory'
      output_dir="$2"
      shift 2
      ;;
    --repo-root)
      (($# >= 2)) || die '--repo-root requires a directory'
      repo_root="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: assemble-release.sh --artifacts-dir DIR --version VERSION --output-dir DIR [--repo-root DIR]'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$artifacts_dir" && -d "$artifacts_dir" ]] || die '--artifacts-dir must be an existing directory'
[[ -n "$version" ]] || die '--version is required'
[[ -n "$output_dir" ]] || die '--output-dir is required'
[[ -d "$repo_root" ]] || die "repository root is missing: $repo_root"
validate_version "$version"
[[ "${#RELEASE_TARGETS[@]}" -eq 7 ]] || die 'release target manifest must contain seven targets'
[[ ! -e "$output_dir" ]] || die "output directory already exists: $output_dir"

mkdir -p "$(dirname "$output_dir")"
stage_parent="$(mktemp -d "${TMPDIR:-/tmp}/neomax-release.XXXXXX")"
stage="$stage_parent/release"
mkdir -p "$stage"
cleanup() {
  rm -rf "$stage_parent"
}
trap cleanup EXIT

archives=()
while IFS= read -r archive; do
  archives+=("$archive")
done < <(find "$artifacts_dir" -type f \( -name '*.tar.gz' -o -name '*.zip' \) -print | sort)
((${#archives[@]} > 0)) || die 'no package archives were downloaded'

artifact_files=()
while IFS= read -r artifact_file; do
  artifact_files+=("$artifact_file")
done < <(find "$artifacts_dir" -type f -print | sort)
[[ "${#artifact_files[@]}" -eq 14 ]] || die "expected exactly fourteen package artifact files, found ${#artifact_files[@]}"
for artifact_file in "${artifact_files[@]}"; do
  case "$(basename "$artifact_file")" in
    SHA256SUMS|*.tar.gz|*.zip) ;;
    *) die "unexpected package artifact file: $artifact_file" ;;
  esac
done

for archive in "${archives[@]}"; do
  basename_archive="$(basename "$archive")"
  known=0
  for target in "${RELEASE_TARGETS[@]}"; do
    [[ "$basename_archive" == "$(archive_name "$version" "$target")" ]] || continue
    known=1
    break
  done
  [[ "$known" -eq 1 ]] || die "unexpected release archive: $basename_archive"
done

release_archives=()
release_targets=()
for target in "${RELEASE_TARGETS[@]}"; do
  expected="$(archive_name "$version" "$target")"
  matches=()
  while IFS= read -r archive; do
    matches+=("$archive")
  done < <(find "$artifacts_dir" -type f -name "$expected" -print | sort)
  [[ "${#matches[@]}" -eq 1 ]] || die "expected exactly one archive for $target, found ${#matches[@]}"

  archive="${matches[0]}"
  bash "$SCRIPT_DIR/check-package.sh" --archive "$archive" --version "$version" --target "$target" >/dev/null

  checksum_files=()
  while IFS= read -r checksum_file; do
    checksum_files+=("$checksum_file")
  done < <(find "$(dirname "$archive")" -maxdepth 1 -type f -name SHA256SUMS -print | sort)
  [[ "${#checksum_files[@]}" -eq 1 ]] || die "expected one per-artifact SHA256SUMS beside $expected"
  checksum_record="$(awk -v name="$expected" '$2 == name { count += 1; hash = $1 } END { printf "%d:%s", count, hash }' "${checksum_files[0]}")"
  checksum_count="${checksum_record%%:*}"
  checksum_value="${checksum_record#*:}"
  case "$checksum_value" in
    \\*) checksum_value="${checksum_value#?}" ;;
  esac
  [[ "$checksum_count" -eq 1 && "$checksum_value" =~ ^[0-9a-f]{64}$ ]] || die "invalid per-artifact checksum for $expected"
  [[ "$checksum_value" == "$(sha256_file "$archive")" ]] || die "per-artifact checksum mismatch for $expected"

  destination="$stage/$expected"
  cp "$archive" "$destination"
  release_archives+=("$destination")
  release_targets+=("$target")
done

bash "$SCRIPT_DIR/checksums.sh" --output "$stage/SHA256SUMS" "${release_archives[@]}" >/dev/null
[[ "$(wc -l < "$stage/SHA256SUMS" | tr -d '[:space:]')" -eq 7 ]] || die 'canonical SHA256SUMS must contain seven records'
duplicate_checksum_names="$(awk '{ counts[$2] += 1 } END { for (name in counts) if (counts[name] != 1) print name }' "$stage/SHA256SUMS")"
[[ -z "$duplicate_checksum_names" ]] || die 'canonical SHA256SUMS contains duplicate asset names'

[[ -f "$repo_root/LICENSE" ]] || die 'repository LICENSE is missing'
cp "$repo_root/LICENSE" "$stage/LICENSE"

for asset in install.sh install.ps1; do
  [[ -f "$repo_root/$asset" && ! -L "$repo_root/$asset" ]] || die "required release installer is missing or unsafe: $asset"
  cp "$repo_root/$asset" "$stage/$asset"
done

notes="$stage/RELEASE-NOTES.md"
{
  printf '# Neomax %s\n\n' "$version"
  printf 'This release contains the verified native packages for all supported targets.\n\n'
  printf '## Packages\n\n'
  printf '| Target | Archive | SHA256 |\n'
  printf '| --- | --- | --- |\n'
  for index in "${!release_targets[@]}"; do
    target="${release_targets[$index]}"
    archive="${release_archives[$index]}"
    printf '| %s | %s | `%s` |\n' "$target" "$(basename "$archive")" "$(sha256_file "$archive")"
  done
  printf '\n## Installation\n\n'
  printf 'Download the package for your operating system and architecture, extract it, then run `bin/neomax install`.\n'
  printf 'The SHA256SUMS file covers every package archive in this release.\n'
} > "$notes"

manifest="$stage/RELEASE-ASSET-MANIFEST.json"
{
  printf '{\n'
  printf '  "schema_version": 1,\n'
  printf '  "product": "%s",\n' "$PRODUCT"
  printf '  "version": "%s",\n' "$version"
  printf '  "asset_count": 13,\n'
  printf '  "archive_count": 7,\n'
  printf '  "archives": [\n'
  for index in "${!release_targets[@]}"; do
    target="${release_targets[$index]}"
    archive="${release_archives[$index]}"
    comma=,
    ((index == ${#release_targets[@]} - 1)) && comma=
    printf '    {"target":"%s","name":"%s","sha256":"%s","size":%s}%s\n' \
      "$target" "$(basename "$archive")" "$(sha256_file "$archive")" "$(file_size "$archive")" "$comma"
  done
  printf '  ],\n'
  printf '  "supporting_files": ["SHA256SUMS", "LICENSE", "RELEASE-NOTES.md", "RELEASE-ASSET-MANIFEST.json", "install.sh", "install.ps1"]\n'
  printf '}\n'
} > "$manifest"

assert_exact_release_assets "$stage" "$version"
bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$stage" --version "$version" >/dev/null

mv "$stage" "$output_dir"
trap - EXIT
printf '%s\n' "$output_dir"
