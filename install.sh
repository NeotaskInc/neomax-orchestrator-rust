#!/usr/bin/env bash
set -euo pipefail

# This bootstrapper only downloads and verifies a release package. The package
# owns installation behavior; this script never invokes a provider.
umask 077

PRODUCT="neomax"
DEFAULT_REPOSITORY="NeotaskInc/neomax-orchestrator-rust"
TEMP_ROOT=""

die() {
  printf 'neomax installer: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
usage: install.sh

Environment:
  NEOMAX_VERSION       Release version, with or without the leading v.
  NEOMAX_TARGET        Supported target override for cross-install or testing.
  NEOMAX_REPOSITORY    GitHub owner/repository (default: NeotaskInc/neomax-orchestrator-rust).
  NEOMAX_BASE_URL      Release base directory containing vVERSION directories.
  NEOMAX_LATEST_URL    JSON endpoint containing a tag_name field when VERSION is omitted.
  NEOMAX_ALLOW_HTTP    Set to 1 only for a trusted local HTTP mirror.

NEOMAX_BASE_URL defaults to the GitHub release download directory. For an
offline mirror, set NEOMAX_VERSION and point NEOMAX_BASE_URL at the mirror
directory containing vVERSION/neomax-vVERSION-TARGET.EXT and SHA256SUMS.
The installer never edits shell profiles. It prints the PATH command to use.
EOF
}

cleanup() {
  if [[ -n "$TEMP_ROOT" && -d "$TEMP_ROOT" ]]; then
    rm -rf "$TEMP_ROOT"
  fi
}
trap cleanup EXIT INT TERM

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

validate_repository() {
  [[ "$1" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] ||
    die "invalid GitHub repository: $1"
}

validate_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] ||
    die "invalid release version: $1"
}

normalize_version() {
  local value="$1"
  value="${value#v}"
  validate_version "$value"
  printf '%s\n' "$value"
}

validate_target() {
  case "$1" in
    x86_64-apple-darwin|aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu|x86_64-unknown-linux-musl|aarch64-unknown-linux-musl|x86_64-pc-windows-msvc)
      ;;
    *)
      die "unsupported release target: $1"
      ;;
  esac
}

detect_target() {
  if [[ -n "${NEOMAX_TARGET:-}" ]]; then
    validate_target "$NEOMAX_TARGET"
    [[ "$NEOMAX_TARGET" != *-windows-* ]] || die 'install.sh requires a Unix release target'
    printf '%s\n' "$NEOMAX_TARGET"
    return
  fi

  local system architecture
  system="$(uname -s 2>/dev/null || true)"
  architecture="$(uname -m 2>/dev/null || true)"
  case "$system:$architecture" in
    Darwin:x86_64) printf '%s\n' 'x86_64-apple-darwin' ;;
    Darwin:arm64|Darwin:aarch64) printf '%s\n' 'aarch64-apple-darwin' ;;
    Linux:x86_64|Linux:amd64|Linux:arm64|Linux:aarch64)
      local libc='gnu'
      local ldd_version=''
      if command -v ldd >/dev/null 2>&1; then
        ldd_version="$(ldd --version 2>&1 || true)"
      fi
      if [[ "$ldd_version" == *musl* ]] || compgen -G '/lib/ld-musl-*' >/dev/null 2>&1 || compgen -G '/usr/lib/ld-musl-*' >/dev/null 2>&1; then
        libc='musl'
      fi
      case "$architecture:$libc" in
        x86_64:gnu|amd64:gnu) printf '%s\n' 'x86_64-unknown-linux-gnu' ;;
        x86_64:musl|amd64:musl) printf '%s\n' 'x86_64-unknown-linux-musl' ;;
        arm64:gnu|aarch64:gnu) printf '%s\n' 'aarch64-unknown-linux-gnu' ;;
        arm64:musl|aarch64:musl) printf '%s\n' 'aarch64-unknown-linux-musl' ;;
        *) die "unsupported Linux architecture: $architecture" ;;
      esac
      ;;
    *) die "unsupported host platform or architecture: ${system:-unknown}/${architecture:-unknown}" ;;
  esac
}

validate_url() {
  local url="$1"
  [[ -n "$url" && "$url" != *[$'\r\n ']* ]] || die 'release URLs may not be empty or contain whitespace'
  if [[ "${NEOMAX_TEST_NO_NETWORK:-0}" == 1 && "$url" != file://* ]]; then
    die 'network access is disabled by the hermetic installer test'
  fi
  case "$url" in
    https://*|file://*) ;;
    http://*) [[ "${NEOMAX_ALLOW_HTTP:-0}" == 1 ]] || die 'HTTP mirrors require NEOMAX_ALLOW_HTTP=1' ;;
    *) die "unsupported release URL scheme: $url" ;;
  esac
}

download_file() {
  local url="$1"
  local destination="$2"
  validate_url "$url"
  require_command curl
  local protocols='=https,file'
  [[ "$url" == http://* ]] && protocols='=https,http,file'
  curl --fail --silent --show-error --location --max-redirs 5 \
    --connect-timeout 20 --max-time 600 --proto "$protocols" --proto-redir '=https' \
    -A 'neomax-release-installer' "$url" -o "$destination"
}

download_text() {
  local url="$1"
  local destination
  destination="$TEMP_ROOT/metadata"
  download_file "$url" "$destination"
  [[ -s "$destination" ]] || die "release metadata is empty: $url"
  cat "$destination"
}

resolve_version() {
  if [[ -n "${NEOMAX_VERSION:-}" ]]; then
    normalize_version "$NEOMAX_VERSION"
    return
  fi

  local repository="${NEOMAX_REPOSITORY:-$DEFAULT_REPOSITORY}"
  validate_repository "$repository"
  local latest_url="${NEOMAX_LATEST_URL:-https://api.github.com/repos/$repository/releases/latest}"
  local metadata tag
  metadata="$(download_text "$latest_url")"
  tag="$(printf '%s\n' "$metadata" | sed -nE 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -n 1)"
  [[ -n "$tag" ]] || die "latest release metadata has no tag_name: $latest_url"
  normalize_version "$tag"
}

release_base() {
  local repository="${NEOMAX_REPOSITORY:-$DEFAULT_REPOSITORY}"
  validate_repository "$repository"
  if [[ -n "${NEOMAX_BASE_URL:-}" ]]; then
    printf '%s\n' "${NEOMAX_BASE_URL%/}"
  else
    printf '%s\n' "https://github.com/$repository/releases/download"
  fi
}

archive_extension() {
  case "$1" in
    *-windows-*) printf '%s\n' zip ;;
    *) printf '%s\n' tar.gz ;;
  esac
}

sha256_file() {
  local output hash
  if command -v shasum >/dev/null 2>&1; then
    output="$(shasum -a 256 "$1")"
  elif command -v sha256sum >/dev/null 2>&1; then
    output="$(sha256sum "$1")"
  elif command -v openssl >/dev/null 2>&1; then
    output="$(openssl dgst -sha256 "$1" | sed -E 's/^.*=[[:space:]]*//')"
  else
    die 'shasum, sha256sum, or openssl is required to verify the release'
  fi
  hash="${output%%[[:space:]]*}"
  case "$hash" in
    \\*) hash="${hash#?}" ;;
  esac
  [[ "$hash" =~ ^[0-9A-Fa-f]{64}$ ]] || die "invalid checksum output for $1"
  printf '%s\n' "$hash" | tr '[:upper:]' '[:lower:]'
}

checksum_for_archive() {
  local checksums="$1"
  local archive_name="$2"
  local expected
  expected="$(awk -v target="$archive_name" '
    length($1) == 64 && $1 !~ /[^0-9A-Fa-f]/ && ($2 == target || $2 == "*" target) {
      if (found != "") exit 2
      found = tolower($1)
    }
    END {
      if (found == "") exit 3
      print found
    }
  ' "$checksums")" || die "SHA256SUMS has no unique entry for $archive_name"
  printf '%s\n' "$expected"
}

verify_archive_checksum() {
  local archive="$1"
  local checksums="$2"
  local expected actual
  expected="$(checksum_for_archive "$checksums" "$(basename "$archive")")"
  actual="$(sha256_file "$archive")"
  [[ "$actual" == "$expected" ]] || die "checksum mismatch for $(basename "$archive")"
}

archive_entry_is_safe() {
  local entry="${1%/}"
  [[ -n "$entry" ]] || return 0
  [[ "$entry" != /* && "$entry" != *'\'* ]] || return 1
  local component
  local old_ifs="$IFS"
  IFS=/
  read -ra components <<< "$entry"
  IFS="$old_ifs"
  for component in "${components[@]}"; do
    [[ -n "$component" && "$component" != '.' && "$component" != '..' ]] || return 1
  done
  [[ "$entry" == "$ARCHIVE_ROOT" || "$entry" == "$ARCHIVE_ROOT/"* ]]
}

validate_tar_layout() {
  local archive="$1"
  require_command tar
  local entry
  while IFS= read -r entry; do
    archive_entry_is_safe "$entry" || die "unsafe archive entry: $entry"
  done < <(tar -tzf "$archive")

  local listing kind
  while IFS= read -r listing; do
    [[ -n "$listing" ]] || continue
    kind="${listing:0:1}"
    case "$kind" in
      -|d|l) ;;
      *) die "unsupported archive entry type in: $listing" ;;
    esac
    if [[ "$kind" == l ]]; then
      local link_target="${listing##* -> }"
      [[ "$link_target" == neomax ]] || die "unsafe archive symlink target: $link_target"
    fi
  done < <(tar -tvzf "$archive")
}

validate_extracted_layout() {
  local root="$1"
  [[ -d "$root" && ! -L "$root" ]] || die 'release archive has no regular package directory'
  local link target
  while IFS= read -r link; do
    target="$(readlink "$link")"
    [[ "$target" == 'neomax' ]] || die "unsafe package symlink: ${link#"$root/"} -> $target"
  done < <(find "$root" -type l -print)
}

install_bin_path() {
  if [[ -n "${NEOMAX_INSTALL_BIN:-}" ]]; then
    printf '%s\n' "$NEOMAX_INSTALL_BIN"
  elif [[ -n "${NEOMAX_INSTALL_ROOT:-}" ]]; then
    printf '%s\n' "${NEOMAX_INSTALL_ROOT%/}/bin"
  else
    printf '%s\n' "${HOME:?HOME is not set}/.local/bin"
  fi
}

print_completion() {
  local bin_path="$1"
  local uninstall_prefix=''
  if [[ -n "${NEOMAX_INSTALL_ROOT:-}" ]]; then
    uninstall_prefix="NEOMAX_INSTALL_ROOT=$(printf '%q' "$NEOMAX_INSTALL_ROOT") "
  elif [[ -n "${NEOMAX_INSTALL_BIN:-}" ]]; then
    uninstall_prefix="NEOMAX_INSTALL_BIN=$(printf '%q' "$NEOMAX_INSTALL_BIN") "
    [[ -z "${NEOMAX_INSTALL_SHARE:-}" ]] ||
      uninstall_prefix+="NEOMAX_INSTALL_SHARE=$(printf '%q' "$NEOMAX_INSTALL_SHARE") "
  fi
  printf '\nNeomax %s is installed.\n' "$VERSION"
  printf 'The installer did not edit a shell profile. For this shell, run:\n'
  printf '  export PATH=%q:$PATH\n' "$bin_path"
  printf 'For persistent PATH setup, add that line to your shell configuration intentionally.\n'
  printf 'Upgrade later by rerunning this installer with NEOMAX_VERSION set to the desired release.\n'
  printf 'Uninstall with:\n  %s%q/neomax uninstall\n' "$uninstall_prefix" "$bin_path"
}

main() {
  if (($#)); then
    case "$1" in
      -h|--help) usage; return 0 ;;
      *) die "unexpected argument: $1 (use --help)" ;;
    esac
  fi

  require_command uname
  require_command sed
  require_command awk
  require_command find
  require_command mktemp
  require_command basename
  require_command readlink
  TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/neomax-installer.XXXXXX")"
  VERSION="$(resolve_version)"
  TARGET="$(detect_target)"
  ARCHIVE_EXTENSION="$(archive_extension "$TARGET")"
  ARCHIVE_NAME="$PRODUCT-v$VERSION-$TARGET.$ARCHIVE_EXTENSION"
  ARCHIVE_ROOT="$PRODUCT-v$VERSION-$TARGET"
  BASE_URL="$(release_base)"
  ASSET_URL="${BASE_URL%/}/v$VERSION"
  CHECKSUMS="$TEMP_ROOT/SHA256SUMS"
  ARCHIVE="$TEMP_ROOT/$ARCHIVE_NAME"

  printf 'Downloading Neomax %s for %s\n' "$VERSION" "$TARGET"
  download_file "$ASSET_URL/SHA256SUMS" "$CHECKSUMS"
  download_file "$ASSET_URL/$ARCHIVE_NAME" "$ARCHIVE"
  verify_archive_checksum "$ARCHIVE" "$CHECKSUMS"

  STAGE="$TEMP_ROOT/package"
  mkdir -m 700 "$STAGE"
  if [[ "$ARCHIVE_EXTENSION" == tar.gz ]]; then
    validate_tar_layout "$ARCHIVE"
    tar -xzf "$ARCHIVE" -C "$STAGE"
  else
    die 'install.sh cannot install a Windows package; use install.ps1'
  fi
  PACKAGE_ROOT="$STAGE/$ARCHIVE_ROOT"
  validate_extracted_layout "$PACKAGE_ROOT"
  [[ -x "$PACKAGE_ROOT/bin/neomax" ]] || die 'release package is missing an executable bin/neomax'

  printf 'Running the local package installer...\n'
  "$PACKAGE_ROOT/bin/neomax" install
  print_completion "$(install_bin_path)"
}

main "$@"
