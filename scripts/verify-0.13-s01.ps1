# 0.13 core-area screen verification: password prompt and 형광펜 overlay.
# Builds the debug desktop/CLI, writes sample documents and captures the running app.
$ErrorActionPreference = 'Stop'
$Project = Split-Path $PSScriptRoot -Parent
$env:CARGO_TARGET_DIR = Join-Path (Split-Path $Project -Parent) '.build/hwpcraft'
Set-Location $Project
$Out = Join-Path $Project 'output/0.13'
New-Item -ItemType Directory -Force -Path $Out | Out-Null

& cargo +1.98.1 build --locked -p hwpcraft -p hwpcraft-cli 2>&1 | Out-File -Encoding utf8 (Join-Path $Out 'build-debug.log')
if ($LASTEXITCODE -ne 0) { Get-Content (Join-Path $Out 'build-debug.log') -Tail 30; throw 'debug build failed' }
$Cli = Join-Path $env:CARGO_TARGET_DIR 'debug/hwpcraft-cli.exe'
$App = Join-Path $env:CARGO_TARGET_DIR 'debug/hwpcraft.exe'

# 1. Password-protected document → the app must ask for the password.
$Secret = Join-Path $Out 'secret.hwpx'
& $Cli new $Secret '암호로 보호된 문서입니다.' --password '1234'
if ($LASTEXITCODE -ne 0) { throw 'cli new --password failed' }
$p = Start-Process -FilePath $App -ArgumentList @("`"$Secret`"", '--screenshot', "`"$(Join-Path $Out 'password-prompt.png')`"", '--exit-after-screenshot') -PassThru -Wait
"password-prompt exit $($p.ExitCode)"

# 2. 형광펜 document → yellow overlay behind the first six characters.
$Plan = Join-Path $Out 'highlight-plan.json'
@'
[{"command":"text.insert","params":{"text":"형광펜 표시 문장입니다. RHWP는 형광펜을 저장만 하고 HwpCraft가 화면에 그립니다."}},
 {"command":"selection.set","params":{"cursor":{"section":0,"paragraph":0,"offset":6},"anchor":0}},
 {"command":"format.highlight","params":{"color":"#ffff00"}},
 {"command":"selection.set","params":{"cursor":{"section":0,"paragraph":0,"offset":30},"anchor":18}},
 {"command":"format.highlight","params":{"color":"#00ff80"}}]
'@ | Set-Content -Encoding utf8 $Plan
$Highlight = Join-Path $Out 'highlight.hwpx'
& $Cli edit --new $Plan $Highlight | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'cli edit highlight failed' }
$p = Start-Process -FilePath $App -ArgumentList @("`"$Highlight`"", '--screenshot', "`"$(Join-Path $Out 'highlight-overlay.png')`"", '--exit-after-screenshot') -PassThru -Wait
"highlight-overlay exit $($p.ExitCode)"

# 3. Round trip of the protected file through the CLI with the password.
& $Cli --password 1234 text $Secret
Get-ChildItem $Out | Select-Object Name, Length
