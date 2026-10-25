<#
.SYNOPSIS
Builds the HwpCraft Windows installer (Inno Setup 6) from the release binaries.

.DESCRIPTION
Stages the same file set that HwpCraft.ps1 Install ships (executables, licenses,
notices, samples) plus a generated hwpcraft.ico, then compiles
packaging/windows/HwpCraft.iss. Output: output/installer/HwpCraft-<ver>-windows-x64-setup.exe
with a SHA256SUMS.txt beside it.

.PARAMETER Build
Run ./HwpCraft.ps1 Build first (release build of hwpcraft and hwpcraft-cli).

.PARAMETER Iscc
Path to ISCC.exe. Defaults to the PATH entry or the standard Inno Setup 6 folder.
#>
[CmdletBinding()]
param(
    [switch]$Build,
    [string]$Iscc,
    [string]$Toolchain = '1.98.1'
)
$ErrorActionPreference = 'Stop'
$Project = Split-Path $PSScriptRoot -Parent
$Workspace = Split-Path $Project -Parent
$Target = Join-Path $Workspace '.build/hwpcraft'
$Version = (Get-Content -LiteralPath (Join-Path $Project 'VERSION') -Raw).Trim()
if ($Version -notmatch '^\d+\.\d{2}$') { throw 'VERSION must use x.xx format, for example 0.13.' }

if (!$Iscc) {
    $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    $Iscc = if ($cmd) { $cmd.Source } else { 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' }
}
if (!(Test-Path -LiteralPath $Iscc)) { throw "Inno Setup 6 compiler not found: $Iscc (install from https://jrsoftware.org/isinfo.php or pass -Iscc)." }
$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (!$python) { throw 'python is required to render hwpcraft.ico (packaging/windows/make-icon.py).' }

if ($Build) { & (Join-Path $Project 'HwpCraft.ps1') -Action Build -Toolchain $Toolchain }
foreach ($name in @('hwpcraft.exe','hwpcraft-cli.exe')) {
    if (!(Test-Path -LiteralPath (Join-Path $Target "release/$name"))) {
        throw "Missing $Target/release/$name. Run ./HwpCraft.ps1 Build or pass -Build."
    }
}

$Out = Join-Path $Project 'output/installer'
$Stage = Join-Path $Out 'stage'
if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Stage, (Join-Path $Stage 'samples') | Out-Null

foreach ($name in @('hwpcraft.exe','hwpcraft-cli.exe')) {
    Copy-Item -LiteralPath (Join-Path $Target "release/$name") -Destination $Stage
}
foreach ($name in @('README.md','LICENSE','THIRD-PARTY-NOTICES.txt','THIRD-PARTY-DEPENDENCIES.md','VERSION')) {
    Copy-Item -LiteralPath (Join-Path $Project $name) -Destination $Stage
}
Get-ChildItem -LiteralPath (Join-Path $Project 'assets/licenses') -Filter '*.txt' | Copy-Item -Destination $Stage
Copy-Item -LiteralPath (Join-Path $Project 'assets/icons/lucide/LICENSE.txt') -Destination (Join-Path $Stage 'LICENSE-Lucide-ISC.txt')
$samples = Join-Path $Project 'samples'
if (!(Test-Path -LiteralPath (Join-Path $samples 'HwpCraft-시작하기.hwpx'))) {
    New-Item -ItemType Directory -Force -Path $samples | Out-Null
    & (Join-Path $Target 'release/hwpcraft-cli.exe') demo $samples
    if ($LASTEXITCODE -ne 0) { throw 'Could not create the sample document.' }
}
Copy-Item -Path (Join-Path $samples '*') -Destination (Join-Path $Stage 'samples') -Recurse
& $python -I (Join-Path $Project 'packaging/windows/make-icon.py') (Join-Path $Stage 'hwpcraft.ico')
if ($LASTEXITCODE -ne 0) { throw 'Icon rendering failed.' }

$log = Join-Path $Project 'logs/installer.log'
New-Item -ItemType Directory -Force -Path (Split-Path $log -Parent) | Out-Null
& $Iscc "/DAppVersion=$Version" "/DSourceDir=$Stage" "/DOutputDir=$Out" "/Q" (Join-Path $Project 'packaging/windows/HwpCraft.iss') *> $log
if ($LASTEXITCODE -ne 0) { Get-Content -LiteralPath $log -Tail 40; throw "ISCC failed: $log" }

$setup = Join-Path $Out "HwpCraft-$Version-windows-x64-setup.exe"
if (!(Test-Path -LiteralPath $setup)) { throw "Installer not produced: $setup" }
$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $Out 'SHA256SUMS.txt'), "$hash  $(Split-Path $setup -Leaf)`n")
Remove-Item -LiteralPath $Stage -Recurse -Force
[pscustomobject]@{ Installer = $setup; Version = $Version; SizeMB = [math]::Round((Get-Item $setup).Length / 1MB, 1); SHA256 = $hash }
