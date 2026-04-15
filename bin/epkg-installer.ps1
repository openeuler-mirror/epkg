# SPDX-License-Identifier: MulanPSL-2.0+
# Copyright (c) 2024 Huawei Technologies Co., Ltd. All rights reserved.
#
# epkg installer for Windows PowerShell
# Usage:
#   irm https://raw.atomgit.com/openeuler/epkg/raw/master/bin/epkg-installer.ps1 | iex
#   epkg-installer.ps1 -Channel conda -Store auto

param(
    [string]$Channel = "",
    [ValidateSet("shared", "private", "auto")]
    [string]$Store = "auto"
)

$ErrorActionPreference = "Stop"

# Configuration
$Arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
$EpkgCache = Join-Path $env:USERPROFILE ".cache\epkg\downloads\epkg"
$GiteeApiBase = "https://gitee.com/api/v5/repos"
$GiteeOwner = "wu_fengguang"
$GiteeRepo = "epkg"

function Write-Step {
    param([string]$Message)
    Write-Host ">> $Message" -ForegroundColor Cyan
}

function Write-Info {
    param([string]$Message)
    Write-Host $Message
}

function Write-Err {
    param([string]$Message)
    Write-Host "ERROR: $Message" -ForegroundColor Red
}

function Test-Administrator {
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($currentUser)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-GitTree {
    $scriptDir = Split-Path -Parent $PSScriptRoot
    if ($PSScriptRoot -and (Test-Path (Join-Path $scriptDir ".git"))) {
        $debugPath = Join-Path $scriptDir "target\debug\epkg.exe"
        if (Test-Path $debugPath) {
            return $debugPath
        }
    }
    return $null
}

function Get-LatestRelease {
    $apiUrl = "$GiteeApiBase/$GiteeOwner/$GiteeRepo/releases/latest"

    try {
        $response = Invoke-RestMethod -Uri $apiUrl -TimeoutSec 30
        return $response.tag_name
    }
    catch {
        Write-Err "Failed to fetch release info from Gitee API: $apiUrl"
        Write-Err $_.Exception.Message
        exit 1
    }
}

function Download-EpkgAsset {
    param(
        [string]$AssetName,
        [string]$Version
    )

    $binaryUrl = "https://gitee.com/$GiteeOwner/$GiteeRepo/releases/download/$Version/$AssetName"
    $shaUrl = "$binaryUrl.sha256"
    $shaFile = Join-Path $EpkgCache "$AssetName.sha256"
    $binaryFile = Join-Path $EpkgCache $AssetName

    # Download SHA256
    Write-Info "Downloading $AssetName.sha256 ..."
    try {
        Invoke-WebRequest -Uri $shaUrl -OutFile $shaFile -TimeoutSec 30
    }
    catch {
        Write-Err "Failed to download checksum file"
        exit 1
    }

    # Verify SHA file is valid (not HTML error page)
    $shaContent = Get-Content $shaFile -Raw
    if ($shaContent -match "<html|<!DOCTYPE|<body" -or $shaContent -match '^\s*\{') {
        Write-Err "Invalid checksum file (got HTML or JSON error)"
        Remove-Item $shaFile -Force -ErrorAction SilentlyContinue
        exit 1
    }

    # Download binary
    Write-Info "Downloading $AssetName ..."
    try {
        Invoke-WebRequest -Uri $binaryUrl -OutFile $binaryFile -TimeoutSec 300
    }
    catch {
        Write-Err "Failed to download epkg binary"
        exit 1
    }

    # Verify SHA256
    $expectedHash = (Get-Content $shaFile).Split()[0]
    $actualHash = (Get-FileHash $binaryFile -Algorithm SHA256).Hash

    if ($actualHash -ne $expectedHash) {
        Write-Err "Checksum verification failed"
        Write-Err "Expected: $expectedHash"
        Write-Err "Actual:   $actualHash"
        exit 1
    }

    Write-Info "Checksum verified OK"
    return $binaryFile
}

function Test-DuplicateInstall {
    $mainEnv = Join-Path $env:USERPROFILE ".epkg\envs\main"
    if (Test-Path $mainEnv) {
        Write-Host "epkg was already initialized for current user" -ForegroundColor Yellow
        Write-Host "TO upgrade epkg: epkg self upgrade"
        Write-Host "TO uninstall epkg: epkg self remove"
        exit 1
    }
}

# Main
Test-DuplicateInstall

# Check if running from git tree
$localBinary = Test-GitTree
if ($localBinary) {
    Write-Info "Using local binary from git tree: $localBinary"
    $EpkgPath = $localBinary
}
else {
    # Create cache directory
    if (-not (Test-Path $EpkgCache)) {
        New-Item -ItemType Directory -Path $EpkgCache -Force | Out-Null
    }

    Write-Info "Fetching latest release from Gitee..."
    $latestVersion = Get-LatestRelease
    Write-Info "Latest release: $latestVersion"
    Write-Info "Destination: $EpkgCache"

    $assetName = "epkg-windows-$Arch.exe"
    $EpkgPath = Download-EpkgAsset -AssetName $assetName -Version $latestVersion
}

# Build install command
$installArgs = @("self", "install", "--store=$Store")
if ($Channel) {
    $installArgs += "--channel=$Channel"
}

# Show installation mode
Write-Host ""
if (Test-Administrator) {
    Write-Info "Installation mode: shared (system-wide)"
}
else {
    Write-Info "Installation mode: private (user-local)"
}

if ($Channel) {
    Write-Info "Installing epkg with channel: $Channel"
}
Write-Info "Store mode: $Store"

# Run installation
Write-Host ""
& $EpkgPath $installArgs
if ($LASTEXITCODE -ne 0) {
    Write-Err "Installation failed with exit code $LASTEXITCODE"
    exit 1
}

# Completion message
Write-Host ""
Write-Host "=================================================" -ForegroundColor Green
Write-Host "            Installation Complete" -ForegroundColor Green
Write-Host "=================================================" -ForegroundColor Green
Write-Host ""
Write-Info "Usage:"
Write-Info "  epkg search [pattern]  - Search for packages"
Write-Info "  epkg install [pkg]    - Install packages"
Write-Info "  epkg remove [pkg]     - Remove packages"
Write-Info "  epkg list             - List packages"
Write-Info "  epkg update           - Update repo data"
Write-Info "  epkg upgrade          - Upgrade packages"
Write-Info "  epkg --help           - Show detailed help"