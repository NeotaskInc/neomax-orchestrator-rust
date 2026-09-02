#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
source "$SCRIPT_DIR/release-assets.sh"

release_dir=""
version=""
while (($#)); do
  case "$1" in
    --release-dir)
      (($# >= 2)) || die '--release-dir requires a directory'
      release_dir="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: verify-release-assets.sh --release-dir DIR --version VERSION'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$release_dir" ]] || die '--release-dir is required'
[[ -n "$version" ]] || die '--version is required'
assert_exact_release_assets "$release_dir" "$version"

checksum_names="$(mktemp "${TMPDIR:-/tmp}/neomax-checksum-assets.XXXXXX")"
expected_archives="$(mktemp "${TMPDIR:-/tmp}/neomax-archive-assets.XXXXXX")"
trap 'rm -f "$checksum_names" "$expected_archives"' EXIT
awk 'NF == 2 && $1 ~ /^[0-9a-f]{64}$/ { print $2 }' "$release_dir/SHA256SUMS" | sort > "$checksum_names"
for target in "${RELEASE_TARGETS[@]}"; do
  archive_name "$version" "$target"
done | sort > "$expected_archives"
cmp -s "$expected_archives" "$checksum_names" || die 'SHA256SUMS does not name the exact seven archives'

while read -r expected_hash asset_name extra; do
  [[ -z "${extra:-}" && "$expected_hash" =~ ^[0-9a-f]{64}$ ]] || die 'SHA256SUMS contains an invalid record'
  [[ "$expected_hash" == "$(sha256_file "$release_dir/$asset_name")" ]] || die "checksum mismatch for $asset_name"
done < "$release_dir/SHA256SUMS"

python3 - "$release_dir" "$version" "${RELEASE_TARGETS[@]}" <<'PY'
import hashlib
import json
import pathlib
import sys

release_dir = pathlib.Path(sys.argv[1])
version = sys.argv[2]
targets = sys.argv[3:]
manifest = json.loads((release_dir / "RELEASE-ASSET-MANIFEST.json").read_text(encoding="utf-8"))
supporting = [
    "SHA256SUMS",
    "LICENSE",
    "RELEASE-NOTES.md",
    "RELEASE-ASSET-MANIFEST.json",
    "install.sh",
    "install.ps1",
]

if set(manifest) != {"schema_version", "product", "version", "asset_count", "archive_count", "archives", "supporting_files"}:
    raise SystemExit("release manifest has unexpected or missing fields")
if manifest["schema_version"] != 1 or manifest["product"] != "neomax" or manifest["version"] != version:
    raise SystemExit("release manifest identity is incorrect")
if manifest["asset_count"] != 13 or manifest["archive_count"] != 7:
    raise SystemExit("release manifest counts are incorrect")
if manifest["supporting_files"] != supporting:
    raise SystemExit("release manifest supporting files are not exact")
if len(manifest["archives"]) != len(targets):
    raise SystemExit("release manifest archive count is incorrect")

for target, entry in zip(targets, manifest["archives"]):
    extension = "zip" if "-windows-" in target else "tar.gz"
    name = f"neomax-v{version}-{target}.{extension}"
    if set(entry) != {"target", "name", "sha256", "size"}:
        raise SystemExit(f"release manifest fields are incorrect for {target}")
    asset = release_dir / name
    digest = hashlib.sha256(asset.read_bytes()).hexdigest()
    if entry != {"target": target, "name": name, "sha256": digest, "size": asset.stat().st_size}:
        raise SystemExit(f"release manifest metadata is incorrect for {target}")
PY

for target in "${RELEASE_TARGETS[@]}"; do
  archive="$release_dir/$(archive_name "$version" "$target")"
  bash "$SCRIPT_DIR/check-package.sh" \
    --archive "$archive" \
    --version "$version" \
    --target "$target" >/dev/null
done

printf '%s\n' 'release assets verified'
