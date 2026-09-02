[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) {
        throw "install.ps1 test: $Message"
    }
}

function Assert-Throws([scriptblock] $Action, [string] $Message) {
    $threw = $false
    try {
        & $Action
    } catch {
        $threw = $true
    }
    Assert-True $threw $Message
}

function New-ZipFixture([string[]] $Names, [int64[]] $Attributes, [string] $Directory) {
    $path = Join-Path $Directory ("fixture-" + [Guid]::NewGuid().ToString('N') + '.zip')
    $archive = [System.IO.Compression.ZipFile]::Open(
        $path,
        [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        for ($index = 0; $index -lt $Names.Count; $index++) {
            $entry = $archive.CreateEntry($Names[$index])
            if ($Attributes[$index] -ne 0) {
                $entry.ExternalAttributes = [int] $Attributes[$index]
            }
            if (-not $Names[$index].EndsWith('/')) {
                $stream = $entry.Open()
                try {
                    $bytes = [Text.Encoding]::UTF8.GetBytes('fixture')
                    $stream.Write($bytes, 0, $bytes.Length)
                } finally {
                    $stream.Dispose()
                }
            }
        }
    } finally {
        $archive.Dispose()
    }
    return $path
}

function Test-RejectedEntry([string] $Name, [int64] $Attributes, [string] $Directory, [string] $ArchiveRoot) {
    $zipPath = New-ZipFixture @($Name) @($Attributes) $Directory
    $destination = Join-Path $Directory ("rejected-" + [Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($destination) | Out-Null
    try {
        Assert-Throws { Expand-SafeZip $zipPath $destination $ArchiveRoot } "accepted unsafe ZIP entry: $Name"
        $children = @(Get-ChildItem -LiteralPath $destination -Force)
        Assert-True ($children.Count -eq 0) "partially extracted unsafe ZIP entry: $Name"
    } finally {
        Remove-Item -LiteralPath $zipPath -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $destination -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$root = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $root 'install.ps1'
$tokens = $null
$parseErrors = $null
[System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref] $tokens, [ref] $parseErrors) | Out-Null
Assert-True ($parseErrors.Count -eq 0) ('PowerShell parser reported errors: ' + ($parseErrors | Out-String))

# Dot-sourcing loads the pure validation and rendering functions without downloading or invoking a package.
. $scriptPath
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$temporary = Join-Path ([IO.Path]::GetTempPath()) ("neomax-install-ps1-test-" + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temporary) | Out-Null
$archiveRoot = 'neomax-v1.2.3-x86_64-pc-windows-msvc'
try {
    $safeName = $archiveRoot + '/data/Unicode Ω $` ; value.txt'
    $safeZip = New-ZipFixture @(
        "$archiveRoot/",
        "$archiveRoot/bin/",
        "$archiveRoot/bin/neomax.exe",
        $safeName
    ) @(0, 0, 0, 0) $temporary
    $safeDestination = Join-Path $temporary 'safe-extract'
    [IO.Directory]::CreateDirectory($safeDestination) | Out-Null
    Expand-SafeZip $safeZip $safeDestination $archiveRoot
    Assert-True (Test-Path -LiteralPath (Join-Path $safeDestination "$archiveRoot/bin/neomax.exe") -PathType Leaf) 'rejected a safe package archive'
    Assert-True (Test-Path -LiteralPath (Join-Path $safeDestination $safeName) -PathType Leaf) 'failed to preserve a safe Unicode and punctuation path'

    $unsafeNames = @(
        "$archiveRoot/../escape.txt",
        "$archiveRoot/nested/../../escape.txt",
        '/rooted.txt',
        '//server/share/file.txt',
        '\\server\share\file.txt',
        'C:/rooted.txt',
        "$archiveRoot/file.txt:secret",
        "$archiveRoot/CON.txt",
        "$archiveRoot/aux/data.txt",
        "$archiveRoot/trailing./file.txt",
        "$archiveRoot/trailing /file.txt",
        "$archiveRoot/invalid?name.txt"
    )
    foreach ($name in $unsafeNames) {
        Test-RejectedEntry $name 0 $temporary $archiveRoot
    }

    Test-RejectedEntry "$archiveRoot/reparse.txt" 0x0400 $temporary $archiveRoot
    Test-RejectedEntry "$archiveRoot/link" -1610612736 $temporary $archiveRoot
    $duplicateZip = New-ZipFixture @(
        "$archiveRoot/bin/",
        "$archiveRoot/BIN/"
    ) @(0, 0) $temporary
    $duplicateDestination = Join-Path $temporary 'duplicate-extract'
    [IO.Directory]::CreateDirectory($duplicateDestination) | Out-Null
    try {
        Assert-Throws { Expand-SafeZip $duplicateZip $duplicateDestination $archiveRoot } 'accepted case-insensitive duplicate ZIP entries'
    } finally {
        Remove-Item -LiteralPath $duplicateZip -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $duplicateDestination -Recurse -Force -ErrorAction SilentlyContinue
    }

    # Join-Path requires a native provider path when this test runs on Unix.
    if ([IO.Path]::DirectorySeparatorChar -eq '\') {
        $specialPath = 'C:\Users\Apostrophe''s Folder\Unicode Ω\Dollar$`Value\Semi;colon'
    } else {
        $specialPath = [IO.Path]::Combine(
            $temporary,
            "Apostrophe's Folder",
            'Unicode Ω',
            'Dollar$`Value',
            'Semi;colon'
        )
    }
    $oldRoot = [Environment]::GetEnvironmentVariable('NEOMAX_INSTALL_ROOT', 'Process')
    [Environment]::SetEnvironmentVariable('NEOMAX_INSTALL_ROOT', $specialPath, 'Process')
    try {
        $output = @(Show-Completion $specialPath '1.2.3') -join "`n"
        $literal = "'" + $specialPath.Replace("'", "''") + "'"
        $expectedSession = '  $env:Path = ' + $literal + " + ';' + " + '$env:Path'
        $expectedPersistent = '  [Environment]::SetEnvironmentVariable(' +
            "'Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + " +
            $literal + ", 'User')"
        Assert-True $output.Contains($expectedSession) 'session PATH guidance did not use a safe literal'
        Assert-True $output.Contains($expectedPersistent) 'persistent PATH guidance did not use a safe literal'
        $expectedUninstall = '$env:NEOMAX_INSTALL_ROOT = ' + $literal + ';'
        Assert-True $output.Contains($expectedUninstall) 'uninstall guidance did not quote the install root safely'
    } finally {
        [Environment]::SetEnvironmentVariable('NEOMAX_INSTALL_ROOT', $oldRoot, 'Process')
    }
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output 'install.ps1 PowerShell tests passed'
