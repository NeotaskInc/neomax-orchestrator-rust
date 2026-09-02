#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

target=""
version="$(root_version)"
binaries_dir=""
output_dir="$REPO_ROOT/target/dist"
while (($#)); do
  case "$1" in
    --target)
      (($# >= 2)) || die '--target requires a value'
      target="$2"
      shift 2
      ;;
    --version)
      (($# >= 2)) || die '--version requires a value'
      version="$2"
      shift 2
      ;;
    --binaries-dir)
      (($# >= 2)) || die '--binaries-dir requires a directory'
      binaries_dir="$2"
      shift 2
      ;;
    --output-dir)
      (($# >= 2)) || die '--output-dir requires a directory'
      output_dir="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: package.sh --target TARGET [--version VERSION] [--binaries-dir DIR] [--output-dir DIR]'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -n "$target" ]] || die '--target is required'
validate_target "$target"
"$SCRIPT_DIR/check-version.sh" --version "$version" --print >/dev/null
if [[ -z "$binaries_dir" ]]; then
  binaries_dir="$REPO_ROOT/target/$target/release"
fi
[[ -d "$binaries_dir" ]] || die "binary directory is missing: $binaries_dir"

root_name="$(archive_root "$version" "$target")"
stage_parent="$(mktemp -d "${TMPDIR:-/tmp}/neomax-package.XXXXXX")"
stage="$stage_parent/$root_name"
trap 'rm -rf "$stage_parent"' EXIT
mkdir -p "$stage/bin" "$stage/share/neomax/shell" "$stage/share/neomax/workflows" "$stage/share/neomax/agents" "$stage/docs"

main_name="$(binary_name "$target" neomax)"
copy_binary "$binaries_dir/$main_name" "$stage/bin/$main_name"
for alias in "${ALIASES[@]:1}"; do
  alias_name="$(binary_name "$target" "$alias")"
  if is_windows_target "$target"; then
    copy_binary "$binaries_dir/$main_name" "$stage/bin/$alias_name"
  else
    ln -s neomax "$stage/bin/$alias_name"
  fi
done
for auxiliary in "${AUXILIARIES[@]}"; do
  auxiliary_name="$(binary_name "$target" "$auxiliary")"
  copy_binary "$binaries_dir/$auxiliary_name" "$stage/bin/$auxiliary_name"
done

policy_asset="$REPO_ROOT/crates/neomax-core/assets/opencode-model-policy.json"
[[ -f "$policy_asset" ]] || die 'missing OpenCode model policy asset'
[[ -f "$REPO_ROOT/LICENSE" ]] || die 'missing LICENSE'
[[ -f "$REPO_ROOT/README.md" ]] || die 'missing README.md'
[[ -f "$REPO_ROOT/docs/INSTALLATION.md" ]] || die 'missing docs/INSTALLATION.md'
cp "$policy_asset" "$stage/share/neomax/opencode-model-policy.json"
cp "$REPO_ROOT/LICENSE" "$stage/LICENSE"
cp "$REPO_ROOT/README.md" "$stage/README.md"
cp "$REPO_ROOT/docs/INSTALLATION.md" "$stage/docs/INSTALLATION.md"
for shell_asset in "${SHELL_ASSETS[@]}"; do
  source_shell_asset="$REPO_ROOT/assets/shell/$shell_asset"
  [[ -f "$source_shell_asset" ]] || die "missing shell asset: $shell_asset"
  cp "$source_shell_asset" "$stage/share/neomax/shell/$shell_asset"
done
for workflow in "${WORKFLOW_ASSETS[@]}"; do
  source_workflow="$REPO_ROOT/assets/workflows/$workflow.md"
  [[ -f "$source_workflow" ]] || die "missing workflow asset: $source_workflow"
  cp "$source_workflow" "$stage/share/neomax/workflows/$workflow.md"
done
kimi_agent="$REPO_ROOT/assets/kimi/neomax-kimi.md"
[[ -f "$kimi_agent" ]] || die "missing Kimi agent asset: $kimi_agent"
copy_text_lf "$kimi_agent" "$stage/share/neomax/agents/neomax-kimi.md"
"$SCRIPT_DIR/manifest.sh" --root "$stage" --version "$version" --target "$target" >/dev/null

mkdir -p "$output_dir"
extension="$(archive_extension "$target")"
archive="$output_dir/$PRODUCT-v$version-$target.$extension"
rm -f "$archive"
if [[ "$extension" == tar.gz ]]; then
  require_command tar
  tar -czf "$archive" -C "$stage_parent" "$root_name"
else
  if command -v zip >/dev/null 2>&1; then
    (cd "$stage_parent" && zip -qr "$archive" "$root_name")
  elif zip_tool="$(seven_zip 2>/dev/null || true)"; [[ -n "$zip_tool" ]]; then
    (cd "$stage_parent" && "$zip_tool" a -tzip "$archive" "$root_name" >/dev/null)
  else
    die 'zip packaging requires zip or 7z on the build runner'
  fi
fi

"$SCRIPT_DIR/check-package.sh" --archive "$archive" --version "$version" --target "$target" >/dev/null
printf '%s\n' "$archive"
