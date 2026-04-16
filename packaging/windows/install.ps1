[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "kb-cli\bin"),
    [string]$BinaryName = "kb.exe",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "[kb-cli] $Message"
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$candidateBins = @(
    (Join-Path $scriptDir "kb.exe"),
    (Join-Path $scriptDir "kb-cli.exe")
)
$sourceBin = $candidateBins | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

if (-not $sourceBin) {
    throw "No installable binary found (kb.exe or kb-cli.exe). Build/package first."
}

Write-Step "Install directory: $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$targetBin = Join-Path $InstallDir $BinaryName
if ((Test-Path -LiteralPath $targetBin) -and (-not $Force.IsPresent)) {
    Write-Step "Existing install detected, overwriting: $targetBin"
}
Copy-Item -LiteralPath $sourceBin -Destination $targetBin -Force
Write-Step "Binary copied: $targetBin"

$userPathRaw = [Environment]::GetEnvironmentVariable("Path", "User")
if ($null -eq $userPathRaw) {
    $userPathRaw = ""
}

$pathItems = @(
    $userPathRaw.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries) |
        ForEach-Object { $_.Trim() }
)
$installDirNorm = $InstallDir.TrimEnd('\')
$existsInUserPath = $false
foreach ($item in $pathItems) {
    if ([string]::Equals($item.TrimEnd('\'), $installDirNorm, [System.StringComparison]::OrdinalIgnoreCase)) {
        $existsInUserPath = $true
        break
    }
}

if (-not $existsInUserPath) {
    $newPath = if ([string]::IsNullOrWhiteSpace($userPathRaw)) {
        $InstallDir
    }
    else {
        "$userPathRaw;$InstallDir"
    }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Step "Added to user PATH: $InstallDir"
}
else {
    Write-Step "Install directory already exists in user PATH"
}

# Make it available in the current shell session immediately.
if ($env:Path -notlike "*$InstallDir*") {
    $env:Path = "$InstallDir;$env:Path"
}

Write-Host ""
Write-Host "Install complete."
Write-Host "Try: kb --help"
Write-Host "If kb is not recognized, reopen the terminal."
