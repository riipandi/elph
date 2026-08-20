#Requires -Version 5.1
<#
.SYNOPSIS
    Elph installer for Windows (x86_64).
.DESCRIPTION
    Downloads the latest elph Windows release, verifies its SHA-256 checksum, and
    installs elph.exe into the install directory (default:
    %LOCALAPPDATA%\Programs\elph\bin). On Linux and macOS use install.sh instead.
.PARAMETER Version
    Pin a specific version tag, e.g. 0.0.26 (the leading v is optional).
.PARAMETER Canary
    Install the latest -canary pre-release instead of the release channel.
.PARAMETER InstallDir
    Override the install directory (defaults to $env:LOCALAPPDATA\Programs\elph\bin).
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$Canary,
    [string]$InstallDir
)

$ErrorActionPreference = "Stop"

$App = "elph"
$RepoOwner = "riipandi"
$RepoName = "elph"

function Write-Step { param([string]$Message) Write-Host "==> $Message" }
function Write-Warn { param([string]$Message) Write-Warning $Message }
function Write-Die { param([string]$Message) Write-Error $Message; exit 1 }

# Resolve options from parameters, falling back to environment variables
# (ELPH_VERSION, ELPH_CANARY, ELPH_INSTALL_DIR) so `irm ... | iex` can pass them.
$Version = if ($Version) { $Version } else { $env:ELPH_VERSION }
$UseCanary = if ($Canary) { $true } else { $env:ELPH_CANARY -eq "1" -or $env:ELPH_CANARY -eq "true" }
$TargetInstallDir = if ($InstallDir) { $InstallDir } else { $env:ELPH_INSTALL_DIR }

# Default install directory: %LOCALAPPDATA%\Programs\elph\bin
if ([string]::IsNullOrWhiteSpace($TargetInstallDir)) {
    $TargetInstallDir = Join-Path $env:LOCALAPPDATA "Programs\elph\bin"
}
$TargetInstallDir = [System.IO.Path]::GetFullPath($TargetInstallDir)

function Normalize-Tag { param([string]$v)
    if ([string]::IsNullOrWhiteSpace($v) -or $v -eq "latest") { return $null }
    if ($v.StartsWith("v")) { return $v }
    return "v$v"
}

function Resolve-Tag { param([bool]$CanaryFlag)
    $api = "https://api.github.com/repos/$RepoOwner/$RepoName/releases?per_page=100"
    Write-Step "Resolving latest $App release..."
    try {
        $releases = Invoke-RestMethod -Uri $api -Headers @{ "User-Agent" = "elph-install" }
    } catch {
        Write-Die "Failed to fetch GitHub releases from $api"
    }
    # Split into release-channel (v*.*.*) and canary (*-canary) tags. The
    # release channel prefers a v*.*.* build, but falls back to the latest
    # *-canary when no release-channel tag exists yet (project ships canaries).
    $stable = @(); $canary = @()
    foreach ($rel in $releases) {
        $tn = [string]$rel.tag_name
        if ($tn -match '^v\d+\.\d+\.\d+-canary$') { $canary += $tn }
        elseif ($tn -match '^v\d+\.\d+\.\d+$') { $stable += $tn }
    }
    if ($CanaryFlag) {
        $matched = $canary
    } else {
        $matched = if ($stable.Count -gt 0) { $stable } else { $canary }
    }
    if ($matched.Count -eq 0) { return $null }
    $matched = $matched | Sort-Object -Descending {
        $b = $_ -replace '-canary$', ''
        if ($b -match '^v(\d+)\.(\d+)\.(\d+)$') { [version]::new([int]$Matches[1], [int]$Matches[2], [int]$Matches[3]) } else { [version]'0.0.0' }
    }
    return $matched[0]
}

# Resolve version tag
$tag = if ($Version) { Normalize-Tag $Version } else { Resolve-Tag $UseCanary }
if ([string]::IsNullOrWhiteSpace($tag)) {
    Write-Die "No $App releases found on GitHub (prefix: v*). Try -Canary for pre-releases."
}

$versionNum = $tag.TrimStart('v')
$archive = "elph-windows-x86_64.zip"
$archiveUrl = "https://github.com/$RepoOwner/$RepoName/releases/download/$tag/$archive"
$checksumUrl = "https://github.com/$RepoOwner/$RepoName/releases/download/$tag/SHA256SUMS"

Write-Host "==> $App $tag -- windows/x86_64$(if ($UseCanary) { ' (pre-release)' })"

$tmp = New-Item -ItemType Directory -Force -Path (Join-Path $env:TEMP ("elph-install-" + [guid]::NewGuid().ToString("N"))) | Select-Object -ExpandProperty FullName
try {
    $zipPath = Join-Path $tmp $archive
    $sumPath = Join-Path $tmp "SHA256SUMS"

    Write-Step "Downloading $archive..."
    Invoke-WebRequest -Uri $archiveUrl -OutFile $zipPath

    Write-Step "Downloading SHA256SUMS..."
    $sumOk = $true
    try {
        Invoke-WebRequest -Uri $checksumUrl -OutFile $sumPath
    } catch {
        $sumOk = $false
        Write-Warn "Checksum file not found; skipping verification"
    }

    $expected = $null
    if ($sumOk -and (Test-Path $sumPath)) {
        $line = Get-Content $sumPath | Where-Object { $_ -match " $archive`$" } | Select-Object -First 1
        if ($line -match '^([0-9a-fA-F]{64})\s') { $expected = $Matches[1] }
        if (-not $expected) { Write-Die "No checksum found for $archive in SHA256SUMS" }
        Write-Step "Verifying SHA256 checksum..."
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zipPath).Hash.ToLowerInvariant()
        if ($actual -ne $expected.ToLowerInvariant()) {
            Write-Die "Checksum mismatch for $archive -- expected $expected, got $actual"
        }
    }

    Write-Step "Extracting archive..."
    Expand-Archive -LiteralPath $zipPath -DestinationPath $tmp -Force
    $exe = Join-Path $tmp "elph.exe"
    if (-not (Test-Path $exe)) { Write-Die "Binary not found in archive (expected elph.exe)" }

    New-Item -ItemType Directory -Force -Path $TargetInstallDir | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $TargetInstallDir "elph.exe") -Force

    Write-Host ""
    if ($expected) { Write-Host "    Checksum:    $expected" }
    Write-Host "    Binary:      $(Join-Path $TargetInstallDir 'elph.exe')"
    Write-Host "    Size:        $((Get-Item (Join-Path $TargetInstallDir 'elph.exe')).Length) bytes"

    $onPath = ($env:Path -split ';') -contains $TargetInstallDir
    if (-not $onPath) {
        Write-Warn "$TargetInstallDir is not on your PATH"
        Write-Host "  Add this to your PowerShell profile ($PROFILE):"
        Write-Host "  `$env:Path = `"$TargetInstallDir;`$env:Path`""
    }

    Write-Step "Run 'elph --help' to get started."
    Write-Step "Visit https://github.com/$RepoOwner/$RepoName for docs."
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
