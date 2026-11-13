[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $(if ($env:USERPROFILE) { $env:USERPROFILE } else { $env:LOCALAPPDATA }) ".kb-cli\bin"),
    [string]$BinaryName = "kb.exe",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "[kb-cli] $Message"
}

function Resolve-GlobalConfigPath {
    param([Parameter(Mandatory = $true)][string]$InstallDir)

    $installLeaf = Split-Path -Path $InstallDir -Leaf
    if ([string]::Equals($installLeaf, "bin", [System.StringComparison]::OrdinalIgnoreCase)) {
        $parent = Split-Path -Path $InstallDir -Parent
        return (Join-Path $parent "config.json")
    }

    return (Join-Path $InstallDir "config.json")
}

function Ensure-GlobalConfigFile {
    param([Parameter(Mandatory = $true)][string]$ConfigPath)

    $existing = [pscustomobject]@{}
    if (Test-Path -LiteralPath $ConfigPath) {
        try {
            $existing = Get-Content -LiteralPath $ConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
        }
        catch {
            throw "Cannot read existing config: $ConfigPath. The file was left unchanged."
        }
    }
    if ($existing -isnot [pscustomobject]) {
        throw "Existing config must be a JSON object: $ConfigPath"
    }
    $knowledgeBasesRaw = $existing.PSObject.Properties["knowledge_bases"].Value
    $knowledgeBases = @()
    if ($knowledgeBasesRaw -is [string]) {
        if (-not [string]::IsNullOrWhiteSpace($knowledgeBasesRaw)) {
            $knowledgeBases = @($knowledgeBasesRaw)
        }
    }
    elseif ($knowledgeBasesRaw -is [System.Collections.IEnumerable]) {
        $knowledgeBases = @($knowledgeBasesRaw)
    }
    $knowledgeBasesProperty = $existing.PSObject.Properties["knowledge_bases"]
    if ($null -eq $knowledgeBasesProperty) {
        $existing | Add-Member -NotePropertyName "knowledge_bases" -NotePropertyValue $knowledgeBases
    }
    else {
        $knowledgeBasesProperty.Value = $knowledgeBases
    }
    if ($null -eq $existing.PSObject.Properties["llm"]) {
        $existing | Add-Member -NotePropertyName "llm" -NotePropertyValue ([pscustomobject]@{})
    }
    if ($existing.llm -isnot [pscustomobject]) {
        throw "Existing llm config must be a JSON object: $ConfigPath"
    }
    $llmDefaults = [ordered]@{
        enable_semantic            = $false
        model                      = "gpt-4o-mini"
        base_url                   = "https://api.openai.com/v1"
        semantic_flow              = "two_step"
        timeout_secs               = 60
        temperature                = 0.1
        max_output_tokens          = 900
        analysis_max_output_tokens = 700
        max_input_chars            = 12000
        max_chunks                 = 12
        retry_max_attempts         = 3
        retry_backoff_ms           = 600
        agent_loop_max_rounds      = 2
        agent_reflection_enabled   = $true
        agent_min_chunk_coverage   = 0.6
    }
    foreach ($k in $llmDefaults.Keys) {
        if ($null -eq $existing.llm.PSObject.Properties[$k]) {
            $existing.llm | Add-Member -NotePropertyName $k -NotePropertyValue $llmDefaults[$k]
        }
    }

    $configDir = Split-Path -Path $ConfigPath -Parent
    if (-not [string]::IsNullOrWhiteSpace($configDir)) {
        New-Item -ItemType Directory -Force -Path $configDir | Out-Null
    }
    [IO.File]::WriteAllText($ConfigPath, ($existing | ConvertTo-Json -Depth 100) + "`n", [Text.UTF8Encoding]::new($false))
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
function Resolve-SourceBinary {
    param([string]$ScriptDir)

    $candidates = New-Object System.Collections.Generic.List[string]
    $candidates.Add((Join-Path $ScriptDir "kb.exe"))
    $candidates.Add((Join-Path $ScriptDir "kb-cli.exe"))

    # If running the template script inside repo (packaging/windows/install.ps1),
    # also discover binaries from target/release and the latest dist package dir.
    $repoRoot = Resolve-Path (Join-Path $ScriptDir "..\..") -ErrorAction SilentlyContinue
    if ($repoRoot) {
        $repoRootPath = $repoRoot.Path
        $candidates.Add((Join-Path $repoRootPath "target\release\kb-cli.exe"))
        $candidates.Add((Join-Path $repoRootPath "target\release\kb.exe"))

        $distRoot = Join-Path $repoRootPath "dist"
        if (Test-Path -LiteralPath $distRoot) {
            $latestStages = Get-ChildItem -Path $distRoot -Directory -Filter "kb-cli-*-windows-*" -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending
            foreach ($stage in $latestStages) {
                $candidates.Add((Join-Path $stage.FullName "kb.exe"))
                $candidates.Add((Join-Path $stage.FullName "kb-cli.exe"))
            }
        }
    }

    foreach ($path in $candidates) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }
        if (Test-Path -LiteralPath $path) {
            return [pscustomobject]@{
                Source = $path
                Tried  = $candidates
            }
        }
    }

    return [pscustomobject]@{
        Source = $null
        Tried  = $candidates
    }
}

$resolved = Resolve-SourceBinary -ScriptDir $scriptDir
$sourceBin = $resolved.Source

if (-not $sourceBin) {
    $triedLines = ($resolved.Tried | Select-Object -Unique | ForEach-Object { " - $_" }) -join "`n"
    throw @"
No installable binary found.
Tried paths:
$triedLines

Tips:
1) If you just packaged, run installer from dist stage directory, for example:
   .\dist\kb-cli-<version>-windows-x86_64\install.ps1
2) Or build first:
   cargo build -p kb-cli --profile release
"@
}

Write-Step "Using binary source: $sourceBin"

Write-Step "Install directory: $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$targetBin = Join-Path $InstallDir $BinaryName
if ((Test-Path -LiteralPath $targetBin) -and (-not $Force.IsPresent)) {
    Write-Step "Existing install detected, overwriting: $targetBin"
}
Copy-Item -LiteralPath $sourceBin -Destination $targetBin -Force
Write-Step "Binary copied: $targetBin"

$globalConfigPath = Resolve-GlobalConfigPath -InstallDir $InstallDir
Ensure-GlobalConfigFile -ConfigPath $globalConfigPath
Write-Step "Global config ensured: $globalConfigPath"

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
    try {
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Step "Added to user PATH: $InstallDir"
    }
    catch {
        Write-Step "Warning: failed to update user PATH automatically. Add this path manually: $InstallDir"
    }
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
