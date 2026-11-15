param(
    [string]$PackageDir = (Join-Path $PSScriptRoot "..\..\packaging\windows"),
    [switch]$RunCliSmoke
)

$ErrorActionPreference = "Stop"
$PackagePath = (Resolve-Path -LiteralPath $PackageDir).Path
$packageName = if ([IO.File]::Exists($PackagePath)) {
    [IO.Path]::GetFileNameWithoutExtension($PackagePath)
}
else {
    [IO.Path]::GetFileName($PackagePath)
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("kb-installer-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $testRoot | Out-Null
$fixtureDir = Join-Path $testRoot "package"
New-Item -ItemType Directory -Path $fixtureDir | Out-Null
if ([IO.Path]::GetExtension($PackagePath) -eq ".zip") {
    Expand-Archive -LiteralPath $PackagePath -DestinationPath $fixtureDir
    $PackagePath = $fixtureDir
}
elseif ([IO.Directory]::Exists($PackagePath)) {
    $packageRoot = $PackagePath
    Get-ChildItem -LiteralPath $packageRoot -Force | Copy-Item -Destination $fixtureDir -Recurse -Force
    $PackagePath = $fixtureDir
}
else {
    throw "Expected a Windows package directory or zip archive: $PackagePath"
}

if (-not $RunCliSmoke) {
    [IO.File]::WriteAllText((Join-Path $fixtureDir "kb.exe"), "installer fixture")
}
elseif (-not (Test-Path -LiteralPath (Join-Path $PackagePath "kb.exe"))) {
    throw "Release package does not contain kb.exe: $PackagePath"
}
if ($RunCliSmoke) {
    foreach ($requiredFile in @("install.ps1", "uninstall.ps1", "install.cmd", "uninstall.cmd", "INSTALL.md")) {
        if (-not (Test-Path -LiteralPath (Join-Path $PackagePath $requiredFile))) {
            throw "Release package is missing $requiredFile"
        }
    }
}

# Redirect this process's HKCU before the installer accesses the user PATH.
# No real user environment settings are changed.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class InstallerRegistrySandbox {
    [DllImport("advapi32.dll")]
    public static extern int RegOverridePredefKey(IntPtr key, IntPtr replacement);
}
'@
$registryPath = "Software\KbCliInstallerTests\" + [guid]::NewGuid()
$registryRoot = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($registryPath)
$environmentKey = $registryRoot.CreateSubKey("Environment")
$environmentKey.SetValue("Path", "C:\kb-installer-test-sentinel")
$environmentKey.Dispose()
$hkcu = [IntPtr](-2147483647)
$overrideResult = [InstallerRegistrySandbox]::RegOverridePredefKey($hkcu, $registryRoot.Handle.DangerousGetHandle())
if ($overrideResult -ne 0) { throw "RegOverridePredefKey failed: $overrideResult" }

$script:failures = 0
function Assert-That([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Test-Case([string]$Name, [scriptblock]$Body) {
    try {
        & $Body
        Write-Host "PASS: $Name"
    }
    catch {
        $script:failures++
        Write-Host "FAIL: $Name - $_"
    }
}

try {
    Test-Case "new config is UTF-8 without BOM" {
        $installDir = Join-Path $testRoot "fresh\bin"
        & (Join-Path $fixtureDir "install.ps1") -InstallDir $installDir
        $bytes = [IO.File]::ReadAllBytes((Join-Path $testRoot "fresh\config.json"))
        Assert-That (-not ($bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)) "JSON starts with a BOM that serde_json rejects"
    }

    Test-Case "upgrade preserves existing settings and adds missing defaults" {
        $configDir = Join-Path $testRoot "upgrade"
        New-Item -ItemType Directory -Path $configDir | Out-Null
        $configPath = Join-Path $configDir "config.json"
        $existing = '{"knowledge_bases":["D:/existing-kb"],"custom":{"keep":true},"llm":{"key":"test-only-key","model":"\u9a8c\u6536\u6a21\u578b","enable_graph":true,"ask_loop_max_rounds":9}}'
        $expectedModel = ($existing | ConvertFrom-Json).llm.model
        [IO.File]::WriteAllText($configPath, $existing, [Text.UTF8Encoding]::new($false))
        & (Join-Path $fixtureDir "install.ps1") -InstallDir (Join-Path $configDir "bin")
        $config = [IO.File]::ReadAllText($configPath) | ConvertFrom-Json
        Assert-That ($config.llm.key -eq "test-only-key") "Existing llm.key was lost"
        Assert-That ($config.llm.model -eq $expectedModel) "Unicode model was changed"
        Assert-That ($config.llm.enable_graph -eq $true) "Existing graph setting was lost"
        Assert-That ($config.llm.ask_loop_max_rounds -eq 9) "Existing ask setting was lost"
        Assert-That ($config.custom.keep -eq $true) "Unknown root settings were lost"
        Assert-That ($config.knowledge_bases[0] -eq "D:/existing-kb") "Registered KB was lost"
        Assert-That ($config.llm.timeout_secs -eq 60) "Missing default was not added"
    }

    Test-Case "invalid existing JSON fails without overwriting config" {
        $configDir = Join-Path $testRoot "invalid"
        New-Item -ItemType Directory -Path $configDir | Out-Null
        $configPath = Join-Path $configDir "config.json"
        $existing = '{"llm": unfinished'
        [IO.File]::WriteAllText($configPath, $existing)
        $rejected = $false
        try {
            & (Join-Path $fixtureDir "install.ps1") -InstallDir (Join-Path $configDir "bin")
        }
        catch { $rejected = $true }
        Assert-That $rejected "Invalid config was silently accepted"
        Assert-That ([IO.File]::ReadAllText($configPath) -eq $existing) "Invalid config was overwritten"
    }

    Test-Case "legacy knowledge_bases string is normalized to an array" {
        $configDir = Join-Path $testRoot "legacy"
        New-Item -ItemType Directory -Path $configDir | Out-Null
        $configPath = Join-Path $configDir "config.json"
        [IO.File]::WriteAllText($configPath, '{"knowledge_bases":"D:/legacy-kb"}')
        & (Join-Path $fixtureDir "install.ps1") -InstallDir (Join-Path $configDir "bin")
        $config = [IO.File]::ReadAllText($configPath) | ConvertFrom-Json
        Assert-That ($config.knowledge_bases.Count -eq 1 -and $config.knowledge_bases[0] -eq "D:/legacy-kb") "Legacy registration was not preserved"
    }

    Test-Case "reinstall keeps one PATH entry; uninstall keeps config and other files" {
        $installDir = Join-Path $testRoot "repeat\bin"
        $configPath = Join-Path $testRoot "repeat\config.json"
        & (Join-Path $fixtureDir "install.ps1") -InstallDir $installDir
        & (Join-Path $fixtureDir "install.ps1") -InstallDir $installDir
        $entries = @([Environment]::GetEnvironmentVariable("Path", "User").Split(';') | Where-Object { $_ -eq $installDir })
        Assert-That ($entries.Count -eq 1) "Repeated install duplicated PATH"
        $configBefore = [Convert]::ToBase64String([IO.File]::ReadAllBytes($configPath))
        [IO.File]::WriteAllText((Join-Path $installDir "keep.txt"), "keep")
        & (Join-Path $fixtureDir "uninstall.ps1") -InstallDir $installDir
        Assert-That (-not (Test-Path -LiteralPath (Join-Path $installDir "kb.exe"))) "Binary was not removed"
        Assert-That (Test-Path -LiteralPath (Join-Path $installDir "keep.txt")) "Unrelated file was removed"
        Assert-That ([Convert]::ToBase64String([IO.File]::ReadAllBytes($configPath)) -eq $configBefore) "Config was changed during uninstall"
        $entries = @([Environment]::GetEnvironmentVariable("Path", "User").Split(';') | Where-Object { $_ -eq $installDir })
        Assert-That ($entries.Count -eq 0) "Uninstall left PATH entry"
    }

    if ($RunCliSmoke) {
        Test-Case "installed release binary supports init, indexing, ask, and MCP" {
            $installDir = Join-Path $testRoot "release-smoke\bin"
            $configPath = Join-Path $testRoot "release-smoke\config.json"
            $kbPath = Join-Path $installDir "kb.exe"
            if ($packageName -match '^kb-cli-(\d+\.\d+\.\d+(?:[.-][A-Za-z0-9.-]+)?)-windows-') {
                $expectedVersion = "kb $($Matches[1])"
                $expectedTag = "v$($Matches[1])"
            }
            else {
                throw "Cannot infer package version from package: $packageName"
            }
            if ($env:RELEASE_TAG -and $env:RELEASE_TAG -ne $expectedTag) {
                throw "Release tag $env:RELEASE_TAG does not match package $expectedVersion"
            }
            & (Join-Path $fixtureDir "install.ps1") -InstallDir $installDir
            $savedKbEnvironment = @{}
            Get-ChildItem Env: | Where-Object { $_.Name -like 'KB_*' } | ForEach-Object {
                $savedKbEnvironment[$_.Name] = $_.Value
            }
            $workDir = Join-Path $testRoot "release-smoke\kb"
            New-Item -ItemType Directory -Path $workDir | Out-Null
            Push-Location $workDir
            try {
                foreach ($key in $savedKbEnvironment.Keys) { Remove-Item -LiteralPath ("Env:" + $key) }
                $env:KB_GLOBAL_CONFIG = $configPath
                $env:KB_LLM_DISABLE_KEYRING = "1"
                $env:KB_AGENT_ID = "release-acceptance"
                $env:KB_SESSION_ID = "package-smoke"
                $version = & $kbPath --version
                Assert-That ($LASTEXITCODE -eq 0 -and $version -eq $expectedVersion) "Unexpected installed version: $version"
                & $kbPath init --output json | Out-Null
                Assert-That ($LASTEXITCODE -eq 0) "init failed"
                [IO.File]::WriteAllText((Join-Path $workDir "guide.md"), "# Release package smoke`n`nThe packaged kb binary can index this local note.", [Text.UTF8Encoding]::new($false))
                & $kbPath add (Join-Path $workDir "guide.md") guide --output json | Out-Null
                Assert-That ($LASTEXITCODE -eq 0) "add failed"
                & $kbPath update --output json | Out-Null
                Assert-That ($LASTEXITCODE -eq 0) "update failed"
                $answer = & $kbPath ask "What can the packaged binary index?" --output json
                Assert-That ($LASTEXITCODE -eq 0) "ask failed"
                Assert-That (($answer -join "`n").Contains("local note")) "ask did not return the indexed content"

                $requests = @(
                    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"release-smoke","version":"1"}}}',
                    '{"jsonrpc":"2.0","method":"notifications/initialized"}',
                    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}',
                    '{"jsonrpc":"2.0","id":3,"method":"ping","params":{}}'
                )
                $OutputEncoding = [Text.UTF8Encoding]::new($false)
                $mcpOutput = $requests | & $kbPath serve --mcp
                Assert-That ($LASTEXITCODE -eq 0) "MCP server failed"
                $responses = @($mcpOutput | ForEach-Object { $_ | ConvertFrom-Json })
                Assert-That ($responses.Count -eq 3) "MCP should answer the three requests with ids"
                Assert-That ($responses[0].result.protocolVersion -eq "2024-11-05") "MCP initialize failed"
                Assert-That ($responses[1].result.tools.Count -eq 8) "MCP tools/list did not return eight tools"
                Assert-That ($null -ne $responses[2].result) "MCP ping failed"

                $installedConfig = Join-Path $testRoot "release-smoke\config.json"
                $configBeforeUninstall = [Convert]::ToBase64String([IO.File]::ReadAllBytes($installedConfig))
                [IO.File]::WriteAllText((Join-Path $installDir "keep.txt"), "preserve")
                & (Join-Path $fixtureDir "uninstall.ps1") -InstallDir $installDir
                Assert-That (-not (Test-Path -LiteralPath $kbPath)) "Release binary was not uninstalled"
                Assert-That (Test-Path -LiteralPath (Join-Path $installDir "keep.txt")) "Uninstall removed unrelated files"
                Assert-That ([Convert]::ToBase64String([IO.File]::ReadAllBytes($installedConfig)) -eq $configBeforeUninstall) "Uninstall modified installed configuration"
            }
            finally {
                Pop-Location
                Get-ChildItem Env: | Where-Object { $_.Name -like 'KB_*' } | ForEach-Object {
                    Remove-Item -LiteralPath ("Env:" + $_.Name)
                }
                foreach ($entry in $savedKbEnvironment.GetEnumerator()) {
                    Set-Item -LiteralPath ("Env:" + $entry.Key) -Value $entry.Value
                }
            }
        }
    }
}
finally {
    [InstallerRegistrySandbox]::RegOverridePredefKey($hkcu, [IntPtr]::Zero) | Out-Null
    $registryRoot.Dispose()
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($registryPath)
    Remove-Item -LiteralPath $testRoot -Recurse -Force
}

if ($script:failures -ne 0) { throw "$script:failures installer checks failed" }
