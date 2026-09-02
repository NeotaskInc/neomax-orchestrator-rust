[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Product = 'neomax'
$DefaultRepository = 'NeotaskInc/neomax-orchestrator-rust'
$TempRoot = $null

function Stop-Installer([string] $Message) {
    throw "neomax installer: $Message"
}

function Show-Usage {
    @'
usage: .\install.ps1

Environment:
  NEOMAX_VERSION       Release version, with or without the leading v.
  NEOMAX_TARGET        Supported target override for cross-install or testing.
  NEOMAX_REPOSITORY    GitHub owner/repository (default: NeotaskInc/neomax-orchestrator-rust).
  NEOMAX_BASE_URL      Release base directory containing vVERSION directories.
  NEOMAX_LATEST_URL    JSON endpoint containing a tag_name field when VERSION is omitted.
  NEOMAX_ALLOW_HTTP    Set to 1 only for a trusted local HTTP mirror.

NEOMAX_BASE_URL defaults to the GitHub release download directory. For an
offline mirror, set NEOMAX_VERSION and point NEOMAX_BASE_URL at the mirror
directory containing vVERSION\neomax-vVERSION-TARGET.zip and SHA256SUMS.
The installer never edits profiles. It prints the PATH command to use.
'@ | Write-Output
}

function Get-EnvironmentValue([string] $Name) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ($null -eq $value) {
        return ''
    }
    return $value
}

function Test-Repository([string] $Repository) {
    if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
        Stop-Installer "invalid GitHub repository: $Repository"
    }
}

function ConvertTo-Version([string] $Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) {
        Stop-Installer 'release version is empty'
    }
    $version = $Value -replace '^v', ''
    if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$') {
        Stop-Installer "invalid release version: $Value"
    }
    return $version
}

function Test-Target([string] $Target) {
    $supported = @(
        'x86_64-apple-darwin',
        'aarch64-apple-darwin',
        'x86_64-unknown-linux-gnu',
        'aarch64-unknown-linux-gnu',
        'x86_64-unknown-linux-musl',
        'aarch64-unknown-linux-musl',
        'x86_64-pc-windows-msvc'
    )
    if ($supported -notcontains $Target) {
        Stop-Installer "unsupported release target: $Target"
    }
}

function Get-Target {
    $override = Get-EnvironmentValue 'NEOMAX_TARGET'
    if ($override) {
        Test-Target $override
        if ($override -ne 'x86_64-pc-windows-msvc') {
            Stop-Installer 'install.ps1 requires the Windows release target'
        }
        return $override
    }

    $isWindows = $env:OS -eq 'Windows_NT'
    if (-not $isWindows) {
        Stop-Installer 'install.ps1 must run on Windows'
    }
    $architecture = ''
    try {
        $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
    } catch {
        $architecture = (Get-EnvironmentValue 'PROCESSOR_ARCHITECTURE').ToLowerInvariant()
    }
    if ($architecture -in @('x64', 'amd64')) {
        return 'x86_64-pc-windows-msvc'
    }
    Stop-Installer "unsupported Windows architecture: $architecture"
}

function Test-ReleaseUrl([string] $Url) {
    if ([string]::IsNullOrWhiteSpace($Url) -or $Url -match '[\r\n\s]') {
        Stop-Installer 'release URLs may not be empty or contain whitespace'
    }
    if ((Get-EnvironmentValue 'NEOMAX_TEST_NO_NETWORK') -eq '1' -and -not $Url.StartsWith('file://', [StringComparison]::OrdinalIgnoreCase)) {
        Stop-Installer 'network access is disabled by the hermetic installer test'
    }
    if ($Url -match '^https://') {
        return
    }
    if ($Url -match '^file://') {
        return
    }
    if ($Url -match '^http://') {
        if ((Get-EnvironmentValue 'NEOMAX_ALLOW_HTTP') -ne '1') {
            Stop-Installer 'HTTP mirrors require NEOMAX_ALLOW_HTTP=1'
        }
        return
    }
    Stop-Installer "unsupported release URL scheme: $Url"
}

function Get-FileUriPath([string] $Url) {
    try {
        return ([Uri]::new($Url)).LocalPath
    } catch {
        Stop-Installer "invalid file URL: $Url"
    }
}

function Save-RemoteFile([string] $Url, [string] $Destination) {
    Test-ReleaseUrl $Url
    if ($Url.StartsWith('file://', [StringComparison]::OrdinalIgnoreCase)) {
        $source = Get-FileUriPath $Url
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            Stop-Installer "release file does not exist: $source"
        }
        Copy-Item -LiteralPath $source -Destination $Destination -Force
        return
    }
    Add-Type -AssemblyName System.Net.Http
    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $false
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.DefaultRequestHeaders.UserAgent.ParseAdd('neomax-release-installer')
    $current = [Uri]::new($Url)
    try {
        for ($redirect = 0; $redirect -le 5; $redirect++) {
            $response = $client.GetAsync(
                $current,
                [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
            ).GetAwaiter().GetResult()
            $status = [int] $response.StatusCode
            if ($status -in @(301, 302, 303, 307, 308)) {
                try {
                    if ($redirect -eq 5 -or $null -eq $response.Headers.Location) {
                        Stop-Installer "release URL redirected too many times: $current"
                    }
                    $next = [Uri]::new($current, $response.Headers.Location)
                    if ($next.Scheme -ne 'https') {
                        Stop-Installer "release URL redirect must use HTTPS: $next"
                    }
                    $current = $next
                } finally {
                    $response.Dispose()
                }
                continue
            }
            try {
                if (-not $response.IsSuccessStatusCode) {
                    Stop-Installer "release download failed with HTTP ${status}: $current"
                }
                $output = [IO.File]::Open(
                    $Destination,
                    [IO.FileMode]::Create,
                    [IO.FileAccess]::Write,
                    [IO.FileShare]::None
                )
                try {
                    $response.Content.CopyToAsync($output).GetAwaiter().GetResult()
                } finally {
                    $output.Dispose()
                }
            } finally {
                $response.Dispose()
            }
            return
        }
        Stop-Installer "release URL redirected too many times: $Url"
    } catch {
        Stop-Installer "could not download $Url`: $($_.Exception.Message)"
    } finally {
        $client.Dispose()
        $handler.Dispose()
    }
}

function Get-RemoteText([string] $Url) {
    $path = Join-Path $TempRoot 'metadata'
    Save-RemoteFile $Url $path
    if ((Get-Item -LiteralPath $path).Length -eq 0) {
        Stop-Installer "release metadata is empty: $Url"
    }
    return Get-Content -LiteralPath $path -Raw
}

function Resolve-Version {
    $requested = Get-EnvironmentValue 'NEOMAX_VERSION'
    if ($requested) {
        return ConvertTo-Version $requested
    }
    $repository = Get-EnvironmentValue 'NEOMAX_REPOSITORY'
    if (-not $repository) {
        $repository = $DefaultRepository
    }
    Test-Repository $repository
    $latestUrl = Get-EnvironmentValue 'NEOMAX_LATEST_URL'
    if (-not $latestUrl) {
        $latestUrl = "https://api.github.com/repos/$repository/releases/latest"
    }
    $metadata = Get-RemoteText $latestUrl | ConvertFrom-Json
    if ($null -eq $metadata.tag_name) {
        Stop-Installer "latest release metadata has no tag_name: $latestUrl"
    }
    return ConvertTo-Version ([string] $metadata.tag_name)
}

function Resolve-BaseUrl {
    $repository = Get-EnvironmentValue 'NEOMAX_REPOSITORY'
    if (-not $repository) {
        $repository = $DefaultRepository
    }
    Test-Repository $repository
    $base = Get-EnvironmentValue 'NEOMAX_BASE_URL'
    if (-not $base) {
        $base = "https://github.com/$repository/releases/download"
    }
    return $base.TrimEnd('/')
}

function Get-ExpectedHash([string] $ChecksumPath, [string] $ArchiveName) {
    $matches = @()
    foreach ($line in Get-Content -LiteralPath $ChecksumPath) {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -ne 2) {
            continue
        }
        $name = $parts[1]
        if ($name.StartsWith('*')) {
            $name = $name.Substring(1)
        }
        if ($parts[0] -match '^[0-9A-Fa-f]{64}$' -and $name -ceq $ArchiveName) {
            $matches += $parts[0].ToLowerInvariant()
        }
    }
    if ($matches.Count -ne 1) {
        Stop-Installer "SHA256SUMS has no unique entry for $ArchiveName"
    }
    return $matches[0]
}

function Verify-Checksum([string] $ArchivePath, [string] $ChecksumPath, [string] $ArchiveName) {
    $expected = Get-ExpectedHash $ChecksumPath $ArchiveName
    $actual = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -cne $expected) {
        Stop-Installer "checksum mismatch for $ArchiveName"
    }
}

function Test-WindowsReservedName([string] $Name) {
    $normalized = $Name.TrimEnd([char[]] @('.', ' '))
    if ([string]::IsNullOrEmpty($normalized)) {
        Stop-Installer "unsafe archive entry component: $Name"
    }
    $stem = $normalized.Split('.')[0]
    if ($stem -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$') {
        Stop-Installer "archive entry uses a Windows device name: $Name"
    }
}

function Test-ZipEntryComponent([string] $Component, [string] $EntryName) {
    if ([string]::IsNullOrEmpty($Component) -or $Component -eq '.' -or $Component -eq '..') {
        Stop-Installer "unsafe archive entry: $EntryName"
    }
    if ($Component.EndsWith('.') -or $Component.EndsWith(' ')) {
        Stop-Installer "archive entry has a trailing dot or space: $EntryName"
    }
    if ($Component.Contains(':')) {
        Stop-Installer "archive entry contains an alternate data stream name: $EntryName"
    }
    if ($Component.IndexOfAny([char[]] @('<', '>', '"', '|', '?', '*')) -ge 0) {
        Stop-Installer "archive entry contains an invalid Windows name: $EntryName"
    }
    foreach ($character in $Component.ToCharArray()) {
        if ([int] $character -lt 0x20) {
            Stop-Installer "archive entry contains a control character: $EntryName"
        }
    }
    Test-WindowsReservedName $Component
}

function Test-ZipEntryName([string] $Name, [string] $ArchiveRoot) {
    if ([string]::IsNullOrEmpty($Name)) {
        Stop-Installer 'archive contains an empty entry name'
    }
    if ($Name.Contains('\') -or $Name.StartsWith('/') -or $Name.StartsWith('//') -or $Name -match '^[A-Za-z]:') {
        Stop-Installer "unsafe archive entry: $Name"
    }
    $parts = $Name.Split('/')
    $lastPart = $parts.Count - 1
    for ($index = 0; $index -lt $parts.Count; $index++) {
        $part = $parts[$index]
        $trailingDirectorySeparator = $index -eq $lastPart -and $Name.EndsWith('/')
        if ($trailingDirectorySeparator) {
            if ($parts.Count -eq 1) {
                Stop-Installer "unsafe archive entry: $Name"
            }
            continue
        }
        Test-ZipEntryComponent $part $Name
    }
    if ($Name -ne $ArchiveRoot -and -not $Name.StartsWith("$ArchiveRoot/", [StringComparison]::Ordinal)) {
        Stop-Installer "archive entry is outside package root: $Name"
    }
}

function Get-ZipEntryAttributes([object] $Entry) {
    if ($Entry.PSObject.Properties.Name -notcontains 'ExternalAttributes') {
        return [int64] 0
    }
    return [int64] $Entry.ExternalAttributes
}

function Test-ZipEntryAttributes([object] $Entry) {
    $attributes = Get-ZipEntryAttributes $Entry
    $unixMode = ($attributes -shr 16) -band 0xF000
    $reparsePoint = ($attributes -band 0x0400) -ne 0
    if ($reparsePoint -or $unixMode -eq 0xA000) {
        Stop-Installer "archive contains a reparse point or symbolic link: $($Entry.FullName)"
    }
}

function Expand-SafeZip([string] $ArchivePath, [string] $Destination, [string] $ArchiveRoot) {
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $zip.Entries) {
            Test-ZipEntryName $entry.FullName $ArchiveRoot
            Test-ZipEntryAttributes $entry
            $normalizedName = $entry.FullName.TrimEnd('/')
            if (-not $seen.Add($normalizedName)) {
                Stop-Installer "archive contains a duplicate entry: $($entry.FullName)"
            }
        }
        foreach ($entry in $zip.Entries) {
            $relative = $entry.FullName.Replace('/', [IO.Path]::DirectorySeparatorChar)
            $destinationPath = [IO.Path]::GetFullPath((Join-Path $Destination $relative))
            $destinationRoot = [IO.Path]::GetFullPath($Destination).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
            if (-not $destinationPath.StartsWith($destinationRoot, [StringComparison]::OrdinalIgnoreCase)) {
                Stop-Installer "unsafe archive destination: $($entry.FullName)"
            }
            if ($entry.FullName.EndsWith('/')) {
                [IO.Directory]::CreateDirectory($destinationPath) | Out-Null
                continue
            }
            $parent = [IO.Path]::GetDirectoryName($destinationPath)
            [IO.Directory]::CreateDirectory($parent) | Out-Null
            if (Test-Path -LiteralPath $destinationPath) {
                Stop-Installer "archive destination already exists: $($entry.FullName)"
            }
            $input = $entry.Open()
            $output = [IO.File]::Open($destinationPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
            try {
                $input.CopyTo($output)
            } finally {
                $output.Dispose()
                $input.Dispose()
            }
        }
    } finally {
        $zip.Dispose()
    }
}

function Test-ExtractedLayout([string] $PackageRoot) {
    if (-not (Test-Path -LiteralPath $PackageRoot -PathType Container)) {
        Stop-Installer 'release archive has no regular package directory'
    }
    foreach ($item in Get-ChildItem -LiteralPath $PackageRoot -Recurse -Force) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            Stop-Installer "release package contains a reparse point: $($item.FullName)"
        }
    }
    $binary = Join-Path $PackageRoot 'bin\neomax.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        Stop-Installer 'release package is missing bin\neomax.exe'
    }
}

function Get-InstallBinPath {
    $bin = Get-EnvironmentValue 'NEOMAX_INSTALL_BIN'
    if ($bin) {
        return $bin
    }
    $root = Get-EnvironmentValue 'NEOMAX_INSTALL_ROOT'
    if ($root) {
        return Join-Path $root 'bin'
    }
    $localAppData = Get-EnvironmentValue 'LOCALAPPDATA'
    if (-not $localAppData) {
        Stop-Installer 'LOCALAPPDATA is not set'
    }
    return Join-Path $localAppData 'Neomax\bin'
}

function ConvertTo-PowerShellSingleQuotedLiteral([string] $Value) {
    if ($null -eq $Value) {
        return "''"
    }
    if ($Value.IndexOfAny([char[]] @("`r", "`n")) -ge 0) {
        Stop-Installer 'cannot print a PATH command containing a newline'
    }
    return "'" + $Value.Replace("'", "''") + "'"
}

function Show-Completion([string] $BinPath, [string] $Version) {
    $binLiteral = ConvertTo-PowerShellSingleQuotedLiteral $BinPath
    $executableLiteral = ConvertTo-PowerShellSingleQuotedLiteral (Join-Path $BinPath 'neomax.exe')
    $uninstall = '& ' + $executableLiteral + ' uninstall'
    $root = Get-EnvironmentValue 'NEOMAX_INSTALL_ROOT'
    if ($root) {
        $rootLiteral = ConvertTo-PowerShellSingleQuotedLiteral $root
        $uninstall = '$env:NEOMAX_INSTALL_ROOT = ' + $rootLiteral + '; ' + $uninstall
    }
    $sessionPath = '  $env:Path = ' + $binLiteral + " + ';' + " + '$env:Path'
    $persistentPath = '  [Environment]::SetEnvironmentVariable(' +
        "'Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + " +
        $binLiteral + ", 'User')"
    Write-Output ''
    Write-Output "Neomax $Version is installed."
    Write-Output 'The installer did not edit a profile. For this PowerShell session, run:'
    Write-Output $sessionPath
    Write-Output 'For persistent PATH setup, add that directory through Windows Environment Variables or run this intentionally:'
    Write-Output $persistentPath
    Write-Output 'Upgrade later by rerunning this installer with NEOMAX_VERSION set to the desired release.'
    Write-Output "Uninstall with: $uninstall"
}

function Invoke-NeomaxInstaller([string[]] $Arguments) {
    $script:NeomaxInstallerExitCode = 1
    try {
        if ($Arguments.Count -gt 0) {
            if ($Arguments.Count -eq 1 -and $Arguments[0] -in @('-h', '--help')) {
                Show-Usage
                $script:NeomaxInstallerExitCode = 0
                return
            }
            Stop-Installer 'unexpected argument (use --help)'
        }

        $TempRoot = Join-Path ([IO.Path]::GetTempPath()) ("neomax-installer-" + [Guid]::NewGuid().ToString('N'))
        [IO.Directory]::CreateDirectory($TempRoot) | Out-Null
        $Version = Resolve-Version
        $Target = Get-Target
        $ArchiveName = "$Product-v$Version-$Target.zip"
        $ArchiveRoot = "$Product-v$Version-$Target"
        $AssetUrl = "$(Resolve-BaseUrl)/v$Version"
        $ChecksumPath = Join-Path $TempRoot 'SHA256SUMS'
        $ArchivePath = Join-Path $TempRoot $ArchiveName

        Write-Output "Downloading Neomax $Version for $Target"
        Save-RemoteFile "$AssetUrl/SHA256SUMS" $ChecksumPath
        Save-RemoteFile "$AssetUrl/$ArchiveName" $ArchivePath
        Verify-Checksum $ArchivePath $ChecksumPath $ArchiveName

        $Stage = Join-Path $TempRoot 'package'
        [IO.Directory]::CreateDirectory($Stage) | Out-Null
        Expand-SafeZip $ArchivePath $Stage $ArchiveRoot
        $PackageRoot = Join-Path $Stage $ArchiveRoot
        Test-ExtractedLayout $PackageRoot

        Write-Output 'Running the local package installer...'
        & (Join-Path $PackageRoot 'bin\neomax.exe') install
        if ($LASTEXITCODE -ne 0) {
            Stop-Installer "package installer failed with exit code $LASTEXITCODE"
        }
        Show-Completion (Get-InstallBinPath) $Version
        $script:NeomaxInstallerExitCode = 0
        return
    } catch {
        Write-Error $_
        $script:NeomaxInstallerExitCode = 1
        return
    } finally {
        if ($TempRoot -and (Test-Path -LiteralPath $TempRoot)) {
            Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-NeomaxInstaller $args
    exit $script:NeomaxInstallerExitCode
}
