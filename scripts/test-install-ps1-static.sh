#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/install.ps1"

fail() {
  printf 'install.ps1 static test: %s\n' "$*" >&2
  exit 1
}

command -v rg >/dev/null 2>&1 || fail 'ripgrep (rg) is required'

[[ -f "$SCRIPT" ]] || fail 'install.ps1 is missing'

require_literal() {
  local literal="$1"
  rg -Fq -- "$literal" "$SCRIPT" || fail "missing security or parser contract: $literal"
}

require_literal '[CmdletBinding()]'
require_literal 'Set-StrictMode -Version Latest'
require_literal 'function ConvertTo-PowerShellSingleQuotedLiteral'
require_literal 'function Test-ZipEntryComponent'
require_literal 'function Test-ZipEntryAttributes'
require_literal 'ExternalAttributes'
require_literal 'ReparsePoint'
require_literal 'TrimEnd'
require_literal "StartsWith('//')"
require_literal 'Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256'
require_literal 'Expand-SafeZip $ArchivePath $Stage $ArchiveRoot'
require_literal 'AllowAutoRedirect = $false'
require_literal 'ResponseHeadersRead'
require_literal 'StatusCode'
require_literal "redirect must use HTTPS"

checksum_line="$(rg -n -m1 'Verify-Checksum \$ArchivePath \$ChecksumPath \$ArchiveName' "$SCRIPT" | cut -d: -f1)"
expand_line="$(rg -n -m1 'Expand-SafeZip \$ArchivePath \$Stage \$ArchiveRoot' "$SCRIPT" | cut -d: -f1)"
[[ -n "$checksum_line" && -n "$expand_line" && "$checksum_line" -lt "$expand_line" ]] ||
  fail 'checksum verification must precede archive extraction'

if rg -n '``|`"' "$SCRIPT" >/dev/null; then
  fail 'PATH guidance still uses unsafe double-quoted path escaping'
fi

printf '%s\n' 'install.ps1 static tests passed'
