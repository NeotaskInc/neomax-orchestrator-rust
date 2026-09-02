#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
source "$SCRIPT_DIR/release-assets.sh"

release_dir=""
repository=""
tag=""
version=""
expected_sha=""
while (($#)); do
  case "$1" in
    --release-dir)
      (($# >= 2)) || die '--release-dir requires a directory'
      release_dir="$2"
      shift 2
      ;;
    --repository)
      (($# >= 2)) || die '--repository requires OWNER/REPOSITORY'
      repository="$2"
      shift 2
      ;;
    --tag)
      (($# >= 2)) || die '--tag requires a value'
      tag="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --expected-sha)
      (($# >= 2)) || die '--expected-sha requires a commit SHA'
      expected_sha="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: publish-release.sh --release-dir DIR --repository OWNER/REPOSITORY --tag TAG --version VERSION --expected-sha SHA'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ "$repository" == 'NeotaskInc/neomax-orchestrator-rust' ]] || die 'release publication is restricted to NeotaskInc/neomax-orchestrator-rust'
[[ "$tag" == "v$version" ]] || die 'release tag must equal vVERSION'
[[ "$expected_sha" =~ ^[0-9a-f]{40}$ ]] || die 'expected SHA must be a full lowercase commit SHA'
require_command gh
require_command python3
bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$release_dir" --version "$version" >/dev/null

remote_tag_commit() {
  local tag_object tag_type tag_sha
  tag_object="$(gh api "repos/$repository/git/ref/tags/$tag")"
  tag_type="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["object"]["type"])' <<< "$tag_object")"
  tag_sha="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["object"]["sha"])' <<< "$tag_object")"
  while [[ "$tag_type" == tag ]]; do
    tag_object="$(gh api "repos/$repository/git/tags/$tag_sha")"
    tag_type="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["object"]["type"])' <<< "$tag_object")"
    tag_sha="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["object"]["sha"])' <<< "$tag_object")"
  done
  [[ "$tag_type" == commit ]] || die "remote tag does not resolve to a commit: $tag"
  printf '%s\n' "$tag_sha"
}

[[ "$(remote_tag_commit)" == "$expected_sha" ]] || die "remote tag does not resolve to expected commit: $tag"

title="Neomax $version"
if gh release view "$tag" --repo "$repository" >/dev/null 2>&1; then
  [[ "$(gh release view "$tag" --repo "$repository" --json tagName --jq .tagName)" == "$tag" ]] || die 'existing release tag is incorrect'
  gh release edit "$tag" --repo "$repository" --title "$title" --notes-file "$release_dir/RELEASE-NOTES.md" --draft=true --prerelease=false >/dev/null
  stale_assets="$(gh release view "$tag" --repo "$repository" --json assets --jq '.assets[].name')"
  while IFS= read -r stale_asset; do
    [[ -n "$stale_asset" ]] || continue
    gh release delete-asset "$tag" "$stale_asset" --repo "$repository" --yes
  done <<< "$stale_assets"
else
  gh release create "$tag" --repo "$repository" --verify-tag --target "$expected_sha" --title "$title" --notes-file "$release_dir/RELEASE-NOTES.md" --draft --prerelease=false >/dev/null
fi

[[ "$(gh release view "$tag" --repo "$repository" --json isDraft --jq .isDraft)" == true ]] || die 'release must remain draft during asset upload and verification'

assets=()
while IFS= read -r asset_name; do
  assets+=("$release_dir/$asset_name")
done < <(release_asset_names "$version")
[[ "${#assets[@]}" -eq 13 ]] || die 'fixed release manifest must contain thirteen assets'
gh release upload "$tag" --repo "$repository" "${assets[@]}"

remote_names="$(mktemp "${TMPDIR:-/tmp}/neomax-remote-assets.XXXXXX")"
expected_names="$(mktemp "${TMPDIR:-/tmp}/neomax-expected-assets.XXXXXX")"
remote_dir="$(mktemp -d "${TMPDIR:-/tmp}/neomax-remote-release.XXXXXX")"
cleanup() {
  rm -f "$remote_names" "$expected_names"
  rm -rf "$remote_dir"
}
trap cleanup EXIT

release_asset_names "$version" | sort > "$expected_names"
gh release view "$tag" --repo "$repository" --json assets --jq '.assets[].name' | sort > "$remote_names"
cmp -s "$expected_names" "$remote_names" || die 'remote release asset names do not match the fixed manifest'
[[ "$(gh release view "$tag" --repo "$repository" --json name --jq .name)" == "$title" ]] || die 'remote release title is incorrect'
[[ "$(gh release view "$tag" --repo "$repository" --json isDraft --jq .isDraft)" == true ]] || die 'remote release was published before verification'
[[ "$(gh release view "$tag" --repo "$repository" --json isPrerelease --jq .isPrerelease)" == false ]] || die 'remote release is marked as a prerelease'

gh release download "$tag" --repo "$repository" --dir "$remote_dir"
bash "$SCRIPT_DIR/verify-release-assets.sh" --release-dir "$remote_dir" --version "$version" >/dev/null
while IFS= read -r asset_name; do
  cmp -s "$release_dir/$asset_name" "$remote_dir/$asset_name" || die "remote release asset differs from assembled asset: $asset_name"
done < <(release_asset_names "$version")

[[ "$(remote_tag_commit)" == "$expected_sha" ]] || die 'remote release tag changed during publication verification'
gh release edit "$tag" --repo "$repository" --draft=false --prerelease=false >/dev/null
[[ "$(gh release view "$tag" --repo "$repository" --json isDraft --jq .isDraft)" == false ]] || die 'release did not become public after verification'
[[ "$(gh release view "$tag" --repo "$repository" --json isPrerelease --jq .isPrerelease)" == false ]] || die 'published release is marked as a prerelease'
[[ "$(gh release view "$tag" --repo "$repository" --json name --jq .name)" == "$title" ]] || die 'published release title changed'
gh release view "$tag" --repo "$repository" --json assets --jq '.assets[].name' | sort > "$remote_names"
cmp -s "$expected_names" "$remote_names" || die 'published release asset names changed after verification'
gh release view "$tag" --repo "$repository" --json url --jq .url
