# 0.13 integration: lock refresh, clippy, full tests, release build, install, MCP docs.
$ErrorActionPreference = 'Stop'
$Project = Split-Path $PSScriptRoot -Parent
Set-Location $Project
$env:CARGO_TARGET_DIR = Join-Path (Split-Path $Project -Parent) '.build/hwpcraft'
$Logs = Join-Path $Project 'logs'

# Workspace version changed: refresh Cargo.lock members only (no registry updates).
& cargo +1.98.1 metadata --format-version 1 --offline | Out-Null
"metadata exit $LASTEXITCODE"

& cargo +1.98.1 clippy --locked --workspace --all-targets -- -D warnings 2>&1 | Out-File -Encoding utf8 (Join-Path $Logs '0.13-clippy.log')
"clippy exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Logs '0.13-clippy.log') | Select-String -Pattern '^error|-->' | Select-Object -First 20; throw 'clippy failed' }

& cargo +1.98.1 test --locked --workspace 2>&1 | Out-File -Encoding utf8 (Join-Path $Logs '0.13-tests.log')
"tests exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Logs '0.13-tests.log') | Select-String -Pattern 'FAILED|panicked' | Select-Object -First 20; throw 'tests failed' }
$passed = (Get-Content (Join-Path $Logs '0.13-tests.log') | Select-String -Pattern 'test result: ok\. (\d+) passed' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
$ignored = (Get-Content (Join-Path $Logs '0.13-tests.log') | Select-String -Pattern '(\d+) ignored' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
"tests passed=$passed ignored=$ignored"

& pwsh -NoProfile -File (Join-Path $Project 'HwpCraft.ps1') -Action Build
"build exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
& pwsh -NoProfile -File (Join-Path $Project 'HwpCraft.ps1') -Action Install
"install exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { throw 'install failed' }

& (Join-Path $Project 'runtime/hwpcraft-cli.exe') --version
& python -I (Join-Path $Project 'scripts/mcp-command-docs.py') (Join-Path $Project 'runtime/hwpcraft-cli.exe')
"mcp docs exit $LASTEXITCODE"
Get-FileHash (Join-Path $Project 'runtime/hwpcraft.exe'), (Join-Path $Project 'runtime/hwpcraft-cli.exe') -Algorithm SHA256 | Format-Table Hash, Path -AutoSize
