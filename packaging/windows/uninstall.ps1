[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "kb-cli\bin"),
    [string]$BinaryName = "kb.exe"
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "[kb-cli] $Message"
}

$targetBin = Join-Path $InstallDir $BinaryName
if (Test-Path -LiteralPath $targetBin) {
    Remove-Item -LiteralPath $targetBin -Force
    Write-Step "Removed binary: $targetBin"
}
else {
    Write-Step "Binary not found: $targetBin"
}

$userPathRaw = [Environment]::GetEnvironmentVariable("Path", "User")
if ($null -eq $userPathRaw) {
    $userPathRaw = ""
}

$installDirNorm = $InstallDir.TrimEnd('\')
$pathItems = @(
    $userPathRaw.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries) |
        ForEach-Object { $_.Trim() } |
        Where-Object {
            -not [string]::Equals($_.TrimEnd('\'), $installDirNorm, [System.StringComparison]::OrdinalIgnoreCase)
        }
)
$newPath = [string]::Join(";", $pathItems)
[Environment]::SetEnvironmentVariable("Path", $newPath, "User")
Write-Step "Removed install directory from user PATH"

if (Test-Path -LiteralPath $InstallDir) {
    $remain = Get-ChildItem -LiteralPath $InstallDir -Force -ErrorAction SilentlyContinue
    if (-not $remain) {
        Remove-Item -LiteralPath $InstallDir -Force
        Write-Step "Removed empty directory: $InstallDir"
    }
    else {
        Write-Step "Directory not empty, kept: $InstallDir"
    }
}

Write-Host ""
Write-Host "Uninstall complete."
Write-Host "Reopen terminal to refresh PATH."
