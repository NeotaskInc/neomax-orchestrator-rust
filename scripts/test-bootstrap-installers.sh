#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/neomax-bootstrap-test.XXXXXX")"
trap 'rm -rf "$TEMP_ROOT"' EXIT INT TERM

fail() {
  printf 'bootstrap installer test: %s\n' "$*" >&2
  exit 1
}

hash_file() {
  local output hash
  if command -v shasum >/dev/null 2>&1; then
    output="$(shasum -a 256 "$1")"
  else
    output="$(sha256sum "$1")"
  fi
  hash="${output%%[[:space:]]*}"
  case "$hash" in
    \\*) hash="${hash#?}" ;;
  esac
  [[ "$hash" =~ ^[0-9A-Fa-f]{64}$ ]] || return 1
  printf '%s\n' "$hash" | tr '[:upper:]' '[:lower:]'
}

require_file() {
  [[ -f "$1" ]] || fail "missing fixture file: $1"
}

require_file "$ROOT/install.sh"
require_file "$ROOT/install.ps1"
grep -Fq -- "--proto-redir '=https'" "$ROOT/install.sh" ||
  fail 'Unix bootstrap does not constrain redirects to HTTPS'

if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -NonInteractive -File "$ROOT/scripts/test-install-ps1.ps1"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -NonInteractive -File "$ROOT/scripts/test-install-ps1.ps1"
else
  bash "$ROOT/scripts/test-install-ps1-static.sh"
fi

MIRROR="$TEMP_ROOT/mirror"
VERSION='0.1.0'
TARGET='x86_64-unknown-linux-gnu'
PACKAGE_NAME="neomax-v$VERSION-$TARGET"
RELEASE_DIR="$MIRROR/v$VERSION"
mkdir -p "$RELEASE_DIR/$PACKAGE_NAME/bin"

cat > "$RELEASE_DIR/$PACKAGE_NAME/bin/neomax" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == install ]] || exit 31
printf '%s\n' "$*" > "${NEOMAX_BOOTSTRAP_TEST_MARKER:?}"
EOF
chmod 0755 "$RELEASE_DIR/$PACKAGE_NAME/bin/neomax"

ARCHIVE="$RELEASE_DIR/$PACKAGE_NAME.tar.gz"
tar -czf "$ARCHIVE" -C "$RELEASE_DIR" "$PACKAGE_NAME"
printf '%s  %s\n' "$(hash_file "$ARCHIVE")" "$(basename "$ARCHIVE")" > "$RELEASE_DIR/SHA256SUMS"
printf '{"tag_name":"v%s"}\n' "$VERSION" > "$MIRROR/latest.json"

MARKER="$TEMP_ROOT/invocation"
INSTALL_ROOT="$TEMP_ROOT/installed"
output="$TEMP_ROOT/output"
NEOMAX_LATEST_URL="file://$MIRROR/latest.json" \
NEOMAX_BASE_URL="file://$MIRROR" \
NEOMAX_TEST_NO_NETWORK=1 \
NEOMAX_TARGET="$TARGET" \
NEOMAX_INSTALL_ROOT="$INSTALL_ROOT" \
NEOMAX_BOOTSTRAP_TEST_MARKER="$MARKER" \
  "$ROOT/install.sh" > "$output"
[[ "$(<"$MARKER")" == install ]] || fail 'bootstrap did not invoke package installer'
grep -Fq "Neomax $VERSION is installed." "$output" || fail 'bootstrap did not print completion'
grep -Fq 'The installer did not edit a shell profile.' "$output" || fail 'bootstrap profile guidance is missing'

TAMPER_MIRROR="$TEMP_ROOT/tampered-mirror"
TAMPER_RELEASE="$TAMPER_MIRROR/v$VERSION"
mkdir -p "$TAMPER_RELEASE"
TAMPER_ARCHIVE="$TAMPER_RELEASE/$PACKAGE_NAME.tar.gz"
cp "$ARCHIVE" "$TAMPER_ARCHIVE"
printf '%s\n' tampered >> "$TAMPER_ARCHIVE"
printf '%s  %s\n' "$(hash_file "$ARCHIVE")" "$(basename "$TAMPER_ARCHIVE")" > "$TAMPER_RELEASE/SHA256SUMS"
if NEOMAX_VERSION="$VERSION" \
  NEOMAX_BASE_URL="file://$TAMPER_MIRROR" \
  NEOMAX_TEST_NO_NETWORK=1 \
  NEOMAX_TARGET="$TARGET" \
  NEOMAX_INSTALL_ROOT="$TEMP_ROOT/tampered-install" \
  NEOMAX_BOOTSTRAP_TEST_MARKER="$TEMP_ROOT/tampered-marker" \
  "$ROOT/install.sh" >/dev/null 2>&1; then
  fail 'tampered archive unexpectedly passed checksum verification'
fi
[[ ! -e "$TEMP_ROOT/tampered-marker" ]] || fail 'package installer ran after checksum failure'

UNSAFE_MIRROR="$TEMP_ROOT/unsafe-mirror"
UNSAFE_RELEASE="$UNSAFE_MIRROR/v$VERSION"
UNSAFE_SOURCE="$TEMP_ROOT/unsafe-source"
UNSAFE_ROOT="$TEMP_ROOT/unsafe-archive-root"
mkdir -p "$UNSAFE_RELEASE" "$UNSAFE_ROOT/$PACKAGE_NAME"
printf '%s\n' unsafe > "$UNSAFE_SOURCE"
cp "$UNSAFE_SOURCE" "$UNSAFE_ROOT/unsafe-source"
UNSAFE_ARCHIVE="$UNSAFE_RELEASE/$PACKAGE_NAME.tar.gz"
tar -czf "$UNSAFE_ARCHIVE" -C "$UNSAFE_ROOT" "$PACKAGE_NAME/../unsafe-source"
printf '%s  %s\n' "$(hash_file "$UNSAFE_ARCHIVE")" "$(basename "$UNSAFE_ARCHIVE")" > "$UNSAFE_RELEASE/SHA256SUMS"
if NEOMAX_VERSION="$VERSION" \
  NEOMAX_BASE_URL="file://$UNSAFE_MIRROR" \
  NEOMAX_TEST_NO_NETWORK=1 \
  NEOMAX_TARGET="$TARGET" \
  NEOMAX_INSTALL_ROOT="$TEMP_ROOT/unsafe-install" \
  NEOMAX_BOOTSTRAP_TEST_MARKER="$TEMP_ROOT/unsafe-marker" \
  "$ROOT/install.sh" >/dev/null 2>&1; then
  fail 'unsafe archive layout unexpectedly passed validation'
fi
[[ ! -e "$TEMP_ROOT/unsafe-marker" ]] || fail 'package installer ran after unsafe archive rejection'

printf '%s\n' 'bootstrap installer tests passed'
