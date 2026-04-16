[CmdletBinding()]
param(
    [string]$Target = "",
    [string]$Profile = "release",
    [string]$DistDir = "dist",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "[package.ps1] $Message"
}

function Get-WorkspaceVersion {
    param([string]$WorkspaceToml)

    if ($WorkspaceToml -match '(?ms)^\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') {
        return $Matches[1]
    }
    return "0.1.0"
}

function Get-ArchLabel {
    param([string]$TargetTriple)

    if ($TargetTriple -match '^x86_64-') { return "x86_64" }
    if ($TargetTriple -match '^aarch64-') { return "aarch64" }
    if ($TargetTriple -match '^i686-') { return "x86" }

    # RuntimeInformation can be unavailable or partially implemented on older PowerShell/.NET hosts.
    $osArch = $null
    try {
        $runtimeInfo = [System.Runtime.InteropServices.RuntimeInformation]
        if ($null -ne $runtimeInfo) {
            $archValue = $runtimeInfo::OSArchitecture
            if ($null -ne $archValue) {
                $osArch = $archValue.ToString()
            }
        }
    }
    catch {
        $osArch = $null
    }

    if ([string]::IsNullOrWhiteSpace($osArch)) {
        $osArch = $env:PROCESSOR_ARCHITECTURE
    }

    if ([string]::IsNullOrWhiteSpace($osArch)) {
        return "x86_64"
    }

    $osArch = $osArch.ToLowerInvariant()
    if ($osArch -eq "amd64" -or $osArch -eq "x64") { return "x86_64" }
    if ($osArch -eq "arm64") { return "aarch64" }
    if ($osArch -eq "x86" -or $osArch -eq "i386" -or $osArch -eq "i686") { return "x86" }
    return $osArch
}

function Test-WindowsTarget {
    param([string]$TargetTriple)

    if ([string]::IsNullOrWhiteSpace($TargetTriple)) {
        return $true
    }

    return $TargetTriple -match '-windows-'
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$workspaceTomlPath = Join-Path $repoRoot "Cargo.toml"
$workspaceToml = Get-Content -Raw -LiteralPath $workspaceTomlPath
$version = Get-WorkspaceVersion -WorkspaceToml $workspaceToml

if (-not (Test-WindowsTarget -TargetTriple $Target)) {
    throw "package.ps1 only packages Windows targets. Received: $Target. Use scripts/package.sh for Linux artifacts."
}

if (-not [string]::IsNullOrWhiteSpace($Target)) {
    $installedTargets = & rustup target list --installed 2>$null
    if ($LASTEXITCODE -eq 0 -and -not ($installedTargets -contains $Target)) {
        throw "Rust target not installed: $Target. Install with: rustup target add $Target"
    }
}

$arch = Get-ArchLabel -TargetTriple $Target

if (-not $SkipBuild.IsPresent) {
    $buildArgs = @("build", "-p", "kb-cli", "--profile", $Profile)
    if (-not [string]::IsNullOrWhiteSpace($Target)) {
        $buildArgs += @("--target", $Target)
    }

    Write-Step ("Running build: cargo " + ($buildArgs -join " "))
    Push-Location $repoRoot
    try {
        & cargo @buildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code: $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

$binaryPath = if (-not [string]::IsNullOrWhiteSpace($Target)) {
    Join-Path $repoRoot "target\$Target\$Profile\kb-cli.exe"
}
else {
    Join-Path $repoRoot "target\$Profile\kb-cli.exe"
}

if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "Binary not found: $binaryPath"
}

$packageName = "kb-cli-$version-windows-$arch"
$distRoot = Join-Path $repoRoot $DistDir
$stageDir = Join-Path $distRoot $packageName
$zipPath = Join-Path $distRoot "$packageName.zip"

if (Test-Path -LiteralPath $stageDir) {
    Remove-Item -LiteralPath $stageDir -Recurse -Force
}
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $stageDir "kb.exe") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\install.ps1") -Destination (Join-Path $stageDir "install.ps1") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\uninstall.ps1") -Destination (Join-Path $stageDir "uninstall.ps1") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\install.cmd") -Destination (Join-Path $stageDir "install.cmd") -Force
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\uninstall.cmd") -Destination (Join-Path $stageDir "uninstall.cmd") -Force

$notes = @"
# KB-CLI Windows Installer

## Files
- kb.exe
- install.cmd / install.ps1
- uninstall.cmd / uninstall.ps1

## One-click install
Double-click `install.cmd`.

## Command line install
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
"@
$notes | Set-Content -LiteralPath (Join-Path $stageDir "INSTALL.md") -Encoding UTF8

Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force

Write-Step "Package ready:"
Write-Host "  Stage: $stageDir"
Write-Host "  Zip  : $zipPath"
