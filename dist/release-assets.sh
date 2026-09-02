#!/usr/bin/env bash

release_supporting_assets=(
  SHA256SUMS
  LICENSE
  RELEASE-NOTES.md
  RELEASE-ASSET-MANIFEST.json
  install.sh
  install.ps1
)

release_asset_names() {
  local version="$1"
  local target
  validate_version "$version"
  for target in "${RELEASE_TARGETS[@]}"; do
    archive_name "$version" "$target"
  done
  printf '%s\n' "${release_supporting_assets[@]}"
}

assert_exact_release_assets() {
  local release_dir="$1"
  local version="$2"
  local expected_file actual_file
  local expected_list actual_list

  [[ -d "$release_dir" ]] || die "release asset directory is missing: $release_dir"
  expected_list="$(mktemp "${TMPDIR:-/tmp}/neomax-expected-assets.XXXXXX")"
  actual_list="$(mktemp "${TMPDIR:-/tmp}/neomax-actual-assets.XXXXXX")"
  release_asset_names "$version" | sort > "$expected_list"

  while IFS= read -r actual_file; do
    [[ -f "$actual_file" && ! -L "$actual_file" ]] || die "release asset is not a regular file: $actual_file"
    basename "$actual_file"
  done < <(find "$release_dir" -mindepth 1 -maxdepth 1 -print | sort) | sort > "$actual_list"

  if ! cmp -s "$expected_list" "$actual_list"; then
    printf '%s\n' 'expected release assets:' >&2
    sed 's/^/  /' "$expected_list" >&2
    printf '%s\n' 'actual release assets:' >&2
    sed 's/^/  /' "$actual_list" >&2
    rm -f "$expected_list" "$actual_list"
    die 'release asset set does not match the fixed manifest'
  fi

  while IFS= read -r expected_file; do
    [[ -f "$release_dir/$expected_file" && ! -L "$release_dir/$expected_file" ]] || {
      rm -f "$expected_list" "$actual_list"
      die "release asset is missing or unsafe: $expected_file"
    }
  done < "$expected_list"
  rm -f "$expected_list" "$actual_list"
}
