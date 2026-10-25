# 0.14 integration: lock refresh, fmt check per package, clippy, full tests,
# release build, install, MCP docs, hashes, and a check that no local-only
# analysis material is tracked by git.
$ErrorActionPreference = 'Stop'
$Project = Split-Path $PSScriptRoot -Parent
Set-Location $Project
$env:CARGO_TARGET_DIR = Join-Path (Split-Path $Project -Parent) '.build/hwpcraft'
$Logs = Join-Path $Project 'logs'
New-Item -ItemType Directory -Force -Path $Logs | Out-Null

& cargo +1.98.1 metadata --format-version 1 --offline | Out-Null
"metadata exit $LASTEXITCODE"

$packages = @('hwpcraft-doc', 'hwpcraft-formats', 'hwpcraft-render', 'hwpcraft-engine', 'hwpcraft-mcp', 'hwpcraft-ui-egui', 'hwpcraft', 'hwpcraft-cli')
$fmtArgs = @('+1.98.1', 'fmt') + ($packages | ForEach-Object { @('-p', $_) }) + @('--', '--check')
& cargo @fmtArgs 2>&1 | Out-File -Encoding utf8 (Join-Path $Logs '0.14-fmt.log')
"fmt exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Logs '0.14-fmt.log') | Select-String -Pattern '^Diff in' | ForEach-Object { ($_ -split ' at line')[0] } | Sort-Object -Unique; throw 'fmt check failed' }

& cargo +1.98.1 clippy --locked --offline --workspace --all-targets -- -D warnings 2>&1 | Out-File -Encoding utf8 (Join-Path $Logs '0.14-clippy.log')
"clippy exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Logs '0.14-clippy.log') | Select-String -Pattern '^error|-->' | Select-Object -First 20; throw 'clippy failed' }

& cargo +1.98.1 test --locked --offline --workspace 2>&1 | Out-File -Encoding utf8 (Join-Path $Logs '0.14-tests.log')
"tests exit $LASTEXITCODE"
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Logs '0.14-tests.log') | Select-String -Pattern 'FAILED|panicked' | Select-Object -First 20; throw 'tests failed' }
$passed = (Get-Content (Join-Path $Logs '0.14-tests.log') | Select-String -Pattern 'test result: ok\. (\d+) passed' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
$ignored = (Get-Content (Join-Path $Logs '0.14-tests.log') | Select-String -Pattern '(\d+) ignored' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
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

# Local-only analysis material must never be tracked (user rule, 2026-10-25).
$leaks = git ls-files | Select-String -Pattern 'rea[-_]|ghidra|probe|coverage|parity|team-plan|docs/team/|output/reference' -CaseSensitive:$false | Where-Object { $_ -notmatch 'tab_probe\.rs$' }
if ($leaks) { "TRACKED ANALYSIS FILES:"; $leaks; throw 'analysis material is tracked' } else { "no analysis material tracked" }
