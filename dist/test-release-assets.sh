#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

checksum_fixture="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
checksum_output="\\${checksum_fixture}  C:\\runner\\artifact.zip"
[[ "$(normalize_sha256_output "$checksum_output")" == "$checksum_fixture" ]] ||
  die 'Windows checksum escape marker was not normalized'
source "$SCRIPT_DIR/release-assets.sh"

version="$(bash "$SCRIPT_DIR/check-version.sh" --print)"
temporary="$(mktemp -d "${TMPDIR:-/tmp}/neomax-release-test.XXXXXX")"
trap 'rm -rf "$temporary"' EXIT

crlf_source="$temporary/crlf-agent.md"
normalized_source="$temporary/normalized-agent.md"
expected_source="$temporary/expected-agent.md"
printf '%s\r\n' '---' 'name: neomax' '---' '${base_prompt}' > "$crlf_source"
printf '%s\n' '---' 'name: neomax' '---' '${base_prompt}' > "$expected_source"
copy_text_lf "$crlf_source" "$normalized_source"
cmp -s "$expected_source" "$normalized_source" || die 'text asset normalization did not remove CRLF endings'

for target in "${RELEASE_TARGETS[@]}"; do
  binaries="$temporary/binaries/$target"
  artifact_dir="$temporary/artifacts/neomax-$target"
  mkdir -p "$binaries" "$artifact_dir"
  for alias in "${ALIASES[@]}"; do
    printf 'fixture %s %s\n' "$target" "$alias" > "$binaries/$(binary_name "$target" "$alias")"
  done
  for auxiliary in "${AUXILIARIES[@]}"; do
    printf 'fixture %s %s\n' "$target" "$auxiliary" > "$binaries/$(binary_name "$target" "$auxiliary")"
  done
  archive="$(bash "$SCRIPT_DIR/package.sh" --target "$target" --version "$version" --binaries-dir "$binaries" --output-dir "$artifact_dir")"
  bash "$SCRIPT_DIR/checksums.sh" --output "$artifact_dir/SHA256SUMS" "$archive" >/dev/null
done

windows_checksum="$temporary/artifacts/neomax-x86_64-pc-windows-msvc/SHA256SUMS"
sed 's/^/\\/' "$windows_checksum" > "$windows_checksum.tmp"
mv "$windows_checksum.tmp" "$windows_checksum"

output="$temporary/release"
bash "$SCRIPT_DIR/assemble-release.sh" \
  --artifacts-dir "$temporary/artifacts" \
  --version "$version" \
  --output-dir "$output" \
  --repo-root "$ROOT" >/dev/null

[[ "$(find "$output" -maxdepth 1 -type f \( -name '*.tar.gz' -o -name '*.zip' \) | wc -l | tr -d '[:space:]')" -eq 7 ]] || die 'release assembly did not retain seven archives'
[[ "$(wc -l < "$output/SHA256SUMS" | tr -d '[:space:]')" -eq 7 ]] || die 'release assembly did not create seven checksum records'
[[ -f "$output/LICENSE" ]] || die 'release assembly did not include LICENSE'
[[ -f "$output/RELEASE-NOTES.md" ]] || die 'release assembly did not generate release notes'
[[ -f "$output/RELEASE-ASSET-MANIFEST.json" ]] || die 'release assembly did not generate asset manifest'
[[ -f "$output/install.sh" ]] || die 'release assembly did not include the POSIX installer'
[[ -f "$output/install.ps1" ]] || die 'release assembly did not include the PowerShell installer'
grep -Fq '"archive_count": 7' "$output/RELEASE-ASSET-MANIFEST.json" || die 'asset manifest count is incorrect'
grep -Fq '"asset_count": 13' "$output/RELEASE-ASSET-MANIFEST.json" || die 'asset manifest total is incorrect'
assert_exact_release_assets "$output" "$version"
bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$output" --version "$version" >/dev/null

expected_names="$temporary/expected-release-assets"
actual_names="$temporary/actual-release-assets"
release_asset_names "$version" | sort > "$expected_names"
find "$output" -mindepth 1 -maxdepth 1 -type f -exec basename {} \; | sort > "$actual_names"
cmp -s "$expected_names" "$actual_names" || die 'release assembly basenames are not exact'
[[ "$(wc -l < "$actual_names" | tr -d '[:space:]')" -eq 13 ]] || die 'release assembly did not create exactly thirteen assets'

extra_release="$temporary/extra-release"
cp -R "$output" "$extra_release"
printf '%s\n' extra > "$extra_release/unexpected.txt"
if bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$extra_release" --version "$version" >/dev/null 2>&1; then
  die 'release verifier accepted an extra asset'
fi

missing_release="$temporary/missing-release-asset"
cp -R "$output" "$missing_release"
rm -f "$missing_release/install.ps1"
if bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$missing_release" --version "$version" >/dev/null 2>&1; then
  die 'release verifier accepted a missing required installer'
fi

tampered_release="$temporary/tampered-release"
cp -R "$output" "$tampered_release"
printf '%s\n' tampered >> "$tampered_release/$(archive_name "$version" "${RELEASE_TARGETS[0]}")"
if bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$tampered_release" --version "$version" >/dev/null 2>&1; then
  die 'release verifier accepted a checksum mismatch'
fi

bad_manifest_release="$temporary/bad-manifest-release"
cp -R "$output" "$bad_manifest_release"
sed 's/"asset_count": 13/"asset_count": 14/' "$output/RELEASE-ASSET-MANIFEST.json" > "$bad_manifest_release/RELEASE-ASSET-MANIFEST.json"
if bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$bad_manifest_release" --version "$version" >/dev/null 2>&1; then
  die 'release verifier accepted incorrect manifest metadata'
fi

fake_bin="$temporary/fake-bin"
fake_state="$temporary/fake-gh-state"
fake_log="$temporary/fake-gh.log"
mkdir -p "$fake_bin" "$fake_state/assets"
cp "$SCRIPT_DIR/test-fixtures/fake-release-gh.sh" "$fake_bin/gh"
chmod 0755 "$fake_bin/gh"
touch "$fake_state/exists"
printf '%s\n' 'Old title' > "$fake_state/name"
printf '%s\n' false > "$fake_state/draft"
printf '%s\n' true > "$fake_state/prerelease"
printf '%s\n' stale > "$fake_state/assets/stale.txt"
release_sha='0123456789abcdef0123456789abcdef01234567'
PATH="$fake_bin:$PATH" \
FAKE_GH_STATE="$fake_state" \
FAKE_GH_LOG="$fake_log" \
FAKE_GH_SHA="$release_sha" \
FAKE_GH_TAG="v$version" \
bash "$SCRIPT_DIR/publish-release.sh" \
  --release-dir "$output" \
  --repository NeotaskInc/neomax-orchestrator-rust \
  --tag "v$version" \
  --version "$version" \
  --expected-sha "$release_sha" >/dev/null

assert_exact_release_assets "$fake_state/assets" "$version"
bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$fake_state/assets" --version "$version" >/dev/null
[[ "$(cat "$fake_state/draft")" == false ]] || die 'publication transaction did not finish with a public release'
[[ "$(cat "$fake_state/prerelease")" == false ]] || die 'publication transaction left a prerelease'
[[ "$(cat "$fake_state/name")" == "Neomax $version" ]] || die 'publication transaction set the wrong title'
expected_operations="$temporary/expected-operations"
printf '%s\n' \
  'edit draft=true' \
  'delete stale.txt' \
  'upload 13' \
  'download' \
  'edit draft=false' > "$expected_operations"
cmp -s "$expected_operations" "$fake_log" || die 'publication transaction did not preserve draft-first operation order'

fresh_state="$temporary/fresh-gh-state"
fresh_log="$temporary/fresh-gh.log"
mkdir -p "$fresh_state/assets"
PATH="$fake_bin:$PATH" \
FAKE_GH_STATE="$fresh_state" \
FAKE_GH_LOG="$fresh_log" \
FAKE_GH_SHA="$release_sha" \
FAKE_GH_TAG="v$version" \
bash "$SCRIPT_DIR/publish-release.sh" \
  --release-dir "$output" \
  --repository NeotaskInc/neomax-orchestrator-rust \
  --tag "v$version" \
  --version "$version" \
  --expected-sha "$release_sha" >/dev/null
assert_exact_release_assets "$fresh_state/assets" "$version"
printf '%s\n' \
  'create' \
  'upload 13' \
  'download' \
  'edit draft=false' > "$expected_operations"
cmp -s "$expected_operations" "$fresh_log" || die 'new publication did not preserve draft-first operation order'

wrong_sha='fedcba9876543210fedcba9876543210fedcba98'
if PATH="$fake_bin:$PATH" \
  FAKE_GH_STATE="$fake_state" \
  FAKE_GH_LOG="$fake_log" \
  FAKE_GH_SHA="$release_sha" \
  FAKE_GH_TAG="v$version" \
  bash "$SCRIPT_DIR/publish-release.sh" \
    --release-dir "$output" \
    --repository NeotaskInc/neomax-orchestrator-rust \
    --tag "v$version" \
    --version "$version" \
    --expected-sha "$wrong_sha" >/dev/null 2>&1; then
  die 'publication transaction accepted a remote tag at the wrong commit'
fi

duplicate_artifacts="$temporary/duplicate-artifacts"
cp -R "$temporary/artifacts" "$duplicate_artifacts"
first_target="${RELEASE_TARGETS[0]}"
first_archive="$temporary/artifacts/neomax-$first_target/$(archive_name "$version" "$first_target")"

extra_package="$temporary/extra-package"
mkdir -p "$extra_package"
tar -xzf "$first_archive" -C "$extra_package"
package_root_name="$(archive_root "$version" "$first_target")"
printf '%s\n' 'unrelated payload' > "$extra_package/$package_root_name/extra.txt"
extra_archive="$temporary/extra.tar.gz"
tar -czf "$extra_archive" -C "$extra_package" "$package_root_name"
if bash "$SCRIPT_DIR/check-package.sh" \
  --archive "$extra_archive" \
  --version "$version" \
  --target "$first_target" >/dev/null 2>&1; then
  die 'package verifier accepted an unmanifested payload file'
fi

assert_archive_rejected() {
  local archive="$1"
  local archive_target="$2"
  local reason="$3"
  if bash "$SCRIPT_DIR/check-package.sh" \
    --archive "$archive" \
    --version "$version" \
    --target "$archive_target" >/dev/null 2>&1; then
    die "package verifier accepted $reason"
  fi
  if bash "$SCRIPT_DIR/verify-install.sh" \
    --archive "$archive" \
    --version "$version" \
    --target "$archive_target" >/dev/null 2>&1; then
    die "install verifier accepted $reason"
  fi
}

missing_workflow_source="$temporary/missing-workflow-source"
mkdir -p "$missing_workflow_source"
case "$first_archive" in
  *.tar.gz)
    tar -xzf "$first_archive" -C "$missing_workflow_source"
    ;;
  *.zip)
    unzip -q "$first_archive" -d "$missing_workflow_source"
    ;;
  *)
    die "unsupported fixture archive format: $first_archive"
    ;;
esac
rm -f "$missing_workflow_source/$package_root_name/share/neomax/workflows/project.md"
missing_workflow_archive="$temporary/missing-project-workflow.$(archive_extension "$first_target")"
case "$missing_workflow_archive" in
  *.tar.gz)
    tar -czf "$missing_workflow_archive" -C "$missing_workflow_source" "$package_root_name"
    ;;
  *.zip)
    (
      cd "$missing_workflow_source"
      zip -qr "$missing_workflow_archive" "$package_root_name"
    )
    ;;
esac
assert_archive_rejected "$missing_workflow_archive" "$first_target" 'an archive without project.md'

traversal_source="$temporary/traversal-source"
mkdir -p "$traversal_source/package"
printf '%s\n' escape > "$traversal_source/escape.txt"
traversal_archive="$temporary/traversal.tar.gz"
(
  cd "$traversal_source/package"
  tar -czf "$traversal_archive" ../escape.txt
)
assert_archive_rejected "$traversal_archive" "$first_target" 'tar traversal'

symlink_source="$temporary/symlink-source/$package_root_name/bin"
mkdir -p "$symlink_source"
ln -s ../../escape "$symlink_source/cmax"
symlink_archive="$temporary/unsafe-symlink.tar.gz"
tar -czf "$symlink_archive" -C "$temporary/symlink-source" "$package_root_name"
assert_archive_rejected "$symlink_archive" "$first_target" 'unsafe tar symlink'

typed_source="$temporary/typed-source/$package_root_name/bin"
mkdir -p "$typed_source"
printf '%s\n' main > "$typed_source/neomax"
ln "$typed_source/neomax" "$typed_source/cmax"
mkfifo "$typed_source/cdx"
typed_archive="$temporary/unsafe-types.tar.gz"
tar -czf "$typed_archive" -C "$temporary/typed-source" "$package_root_name"
assert_archive_rejected "$typed_archive" "$first_target" 'tar hardlink or device entry'

windows_target='x86_64-pc-windows-msvc'
windows_root_name="$(archive_root "$version" "$windows_target")"
zip_traversal_source="$temporary/zip-traversal-source"
mkdir -p "$zip_traversal_source"
printf '%s\n' escape > "$temporary/zip-escape.txt"
(
  cd "$zip_traversal_source"
  zip -q "$temporary/zip-traversal.zip" ../zip-escape.txt
)
assert_archive_rejected "$temporary/zip-traversal.zip" "$windows_target" 'ZIP traversal'

zip_symlink_source="$temporary/zip-symlink-source/$windows_root_name"
mkdir -p "$zip_symlink_source/bin"
ln -s /tmp "$zip_symlink_source/bin/neomax.exe"
(
  cd "$temporary/zip-symlink-source"
  zip -qyr "$temporary/unsafe-symlink.zip" "$windows_root_name"
)
assert_archive_rejected "$temporary/unsafe-symlink.zip" "$windows_target" 'ZIP symlink'

duplicate_dir="$duplicate_artifacts/duplicate"
mkdir -p "$duplicate_dir"
cp "$first_archive" "$duplicate_dir/"
cp "$temporary/artifacts/neomax-$first_target/SHA256SUMS" "$duplicate_dir/SHA256SUMS"
if bash "$SCRIPT_DIR/assemble-release.sh" \
  --artifacts-dir "$duplicate_artifacts" \
  --version "$version" \
  --output-dir "$temporary/duplicate-release" \
  --repo-root "$ROOT" >/dev/null 2>&1; then
  die 'release assembly accepted a duplicate archive'
fi

missing_artifacts="$temporary/missing-artifacts"
cp -R "$temporary/artifacts" "$missing_artifacts"
rm -f "$missing_artifacts/neomax-${RELEASE_TARGETS[1]}/$(archive_name "$version" "${RELEASE_TARGETS[1]}")"
if bash "$SCRIPT_DIR/assemble-release.sh" \
  --artifacts-dir "$missing_artifacts" \
  --version "$version" \
  --output-dir "$temporary/missing-release" \
  --repo-root "$ROOT" >/dev/null 2>&1; then
  die 'release assembly accepted a missing archive'
fi

printf '%s\n' 'release asset checks passed'
