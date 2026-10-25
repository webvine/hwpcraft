[CmdletBinding()]
param(
    [ValidateSet('Build','Test','Install','Start','Launcher','Status')][string]$Action = 'Start',
    [string]$File,
    [ValidateRange(1,32)][int]$Jobs = 12,
    [string]$Toolchain = '1.98.1'
)
$ErrorActionPreference = 'Stop'
$Project = $PSScriptRoot
$Workspace = Split-Path $Project -Parent
$Target = Join-Path $Workspace '.build/hwpcraft'
$Logs = Join-Path $Project 'logs'
$Runtime = Join-Path $Project 'runtime'
$Version = (Get-Content -LiteralPath (Join-Path $Project 'VERSION') -Raw).Trim()
if ($Version -notmatch '^\d+\.\d{2}$') { throw 'VERSION must use x.xx format, for example 0.10.' }
New-Item -ItemType Directory -Force -Path $Logs,$Runtime | Out-Null

function Write-AtomicJson($Path,$Value) {
    $temp = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    [IO.File]::WriteAllText($temp,($Value | ConvertTo-Json -Depth 15),[Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temp -Destination $Path -Force
}
function Get-AssetLicense([string]$Name) {
    # License texts moved from assets/ to assets/licenses/ on 2026-10-25; accept either place.
    foreach ($dir in @('assets/licenses','assets')) {
        $candidate = Join-Path $Project "$dir/$Name"
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw "Missing license text: assets/licenses/$Name"
}
function Invoke-Cargo([string[]]$Arguments,[string]$Log) {
    $oldTarget = $env:CARGO_TARGET_DIR
    $oldJobs = $env:CARGO_BUILD_JOBS
    try {
        $env:CARGO_TARGET_DIR = $Target
        $env:CARGO_BUILD_JOBS = "$Jobs"
        Push-Location $Project
        & cargo "+$Toolchain" @Arguments *> $Log
        if ($LASTEXITCODE -ne 0) { Get-Content -LiteralPath $Log -Tail 50; throw "Cargo failed: $Log" }
    } finally { Pop-Location; $env:CARGO_TARGET_DIR=$oldTarget; $env:CARGO_BUILD_JOBS=$oldJobs }
}
switch ($Action) {
    'Build' {
        if (!(Test-Path -LiteralPath (Join-Path $Workspace 'rhwp/Cargo.toml'))) { throw 'RHWP source must exist in ../rhwp. No automatic Git operation is performed.' }
        Invoke-Cargo @('build','--release','--locked','-p','hwpcraft','-p','hwpcraft-cli') (Join-Path $Logs 'build.log')
        Write-Host "Build complete: $Target/release"
    }
    'Test' {
        Invoke-Cargo @('test','--locked','--workspace') (Join-Path $Logs 'test.log')
        Write-Host "Tests passed: $Logs/test.log"
    }
    'Install' {
        # Refuse a stale or partly rebuilt release before touching installation
        # files or the Launcher manifest.
        foreach ($name in @('hwpcraft-cli.exe','hwpcraft.exe')) {
            $built = Join-Path $Target "release/$name"
            if (!(Test-Path -LiteralPath $built)) {throw "Build first: ./HwpCraft.ps1 Build (missing $built)"}
            $reported = (& $built --version 2>&1 | Out-String).Trim()
            $buildMatch = [regex]::Match($reported, '^HwpCraft\s+(\d+\.\d{2})(?:\s|$)')
            if ($LASTEXITCODE -ne 0 -or !$buildMatch.Success -or $buildMatch.Groups[1].Value -ne $Version) {
                throw "Release build version mismatch for $name (expected $Version). Run ./HwpCraft.ps1 Build first."
            }
        }
        $Data = Join-Path $Workspace 'runtime/launcher/ArtCraftLauncherData/data'
        # Versioned installs let the launcher switch to a new release while an
        # older window keeps its executable and unsaved document open.
        $InstalledDir = Join-Path $Data "apps/hwpcraft/$Version"
        New-Item -ItemType Directory -Force -Path $InstalledDir | Out-Null
        foreach ($name in @('hwpcraft.exe','hwpcraft-cli.exe')) {
            $source = Join-Path $Target "release/$name"
            if (!(Test-Path -LiteralPath $source)) {throw "Build first: ./HwpCraft.ps1 Build (missing $source)"}
            foreach ($destination in @((Join-Path $Runtime $name),(Join-Path $InstalledDir $name))) {
                $same = (Test-Path -LiteralPath $destination) -and ((Get-FileHash -LiteralPath $source).Hash -eq (Get-FileHash -LiteralPath $destination).Hash)
                if (!$same) {Copy-Item -LiteralPath $source -Destination $destination -Force}
            }
        }
        foreach ($name in @('README.md','LICENSE','THIRD-PARTY-NOTICES.txt','VERSION')) {
            Copy-Item -LiteralPath (Join-Path $Project $name) -Destination (Join-Path $Runtime $name) -Force
            Copy-Item -LiteralPath (Join-Path $Project $name) -Destination (Join-Path $InstalledDir $name) -Force
        }
        foreach ($license in (Get-ChildItem -LiteralPath (Join-Path $Project 'assets/licenses') -Filter '*.txt')) {
            Copy-Item -LiteralPath $license.FullName -Destination (Join-Path $Runtime $license.Name) -Force
            Copy-Item -LiteralPath $license.FullName -Destination (Join-Path $InstalledDir $license.Name) -Force
        }
        Copy-Item -LiteralPath (Join-Path $Project 'assets/icons/lucide/LICENSE.txt') -Destination (Join-Path $Runtime 'LICENSE-Lucide-ISC.txt') -Force
        Copy-Item -LiteralPath (Join-Path $Runtime 'LICENSE-Lucide-ISC.txt') -Destination $InstalledDir -Force
        Copy-Item -LiteralPath (Join-Path $Project 'THIRD-PARTY-DEPENDENCIES.md') -Destination (Join-Path $Runtime 'THIRD-PARTY-DEPENDENCIES.md') -Force
        Copy-Item -LiteralPath (Join-Path $Runtime 'THIRD-PARTY-DEPENDENCIES.md') -Destination $InstalledDir -Force
        $sample = Join-Path $Project 'samples'
        New-Item -ItemType Directory -Force -Path $sample | Out-Null
        if (!(Test-Path -LiteralPath (Join-Path $sample 'HwpCraft-시작하기.hwpx'))) {
            & (Join-Path $Runtime 'hwpcraft-cli.exe') demo $sample
            if ($LASTEXITCODE -ne 0) {throw 'Could not create the sample document.'}
        }
        Copy-Item -LiteralPath $sample -Destination $InstalledDir -Recurse -Force
        $Mirror = Join-Path $Workspace "local-mirror/hwpcraft/$Version"
        New-Item -ItemType Directory -Force -Path $Mirror | Out-Null
        $archive = "hwpcraft-$Version-windows-x64-portable.zip"
        $temporaryArchive = Join-Path $Mirror ".packing-$([guid]::NewGuid().ToString('N')).zip"
        Compress-Archive -Path (Join-Path $InstalledDir '*') -DestinationPath $temporaryArchive
        $hash = (Get-FileHash -LiteralPath $temporaryArchive -Algorithm SHA256).Hash.ToLowerInvariant()
        $complete = Join-Path $Mirror '.complete'
        if (Test-Path -LiteralPath $complete) {Remove-Item -LiteralPath $complete}
        Move-Item -LiteralPath $temporaryArchive -Destination (Join-Path $Mirror $archive) -Force
        [IO.File]::WriteAllText((Join-Path $Mirror 'SHA256SUMS.txt'),"$hash  $archive`n")
        [IO.File]::WriteAllText((Join-Path $Mirror 'NOTES.md'),"HwpCraft $Version. Independent local Rust desktop editor using RHWP (MIT). HWP/HWPX editing, formatting, tables, undo/redo and PDF export. Source: workspace/hwpcraft. No public upstream release.`n")
        [IO.File]::WriteAllText($complete,'')
        $manifest = Join-Path $Data 'installed.json'
        $workspaceHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Workspace))).Substring(0,16)
        $mutex = [Threading.Mutex]::new($false,"Local\CraftManifest-$workspaceHash")
        $locked = $false
        try {
            try {$locked = $mutex.WaitOne([TimeSpan]::FromSeconds(30))} catch [Threading.AbandonedMutexException] {$locked=$true}
            if (!$locked) {throw 'Timed out waiting for launcher manifest.'}
            $installed = if (Test-Path -LiteralPath $manifest) {Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json -AsHashtable} else {@{apps=@{}}}
            $installed.apps['hwpcraft'] = @{
                version=$Version;kind='PortableZip';path=(Join-Path $InstalledDir 'hwpcraft.exe')
                installed_at=[DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
                files=@(Get-ChildItem -LiteralPath $InstalledDir -Recurse -File | ForEach-Object {$_.FullName})
                needs_root=$false
            }
            Write-AtomicJson $manifest $installed
        } finally {if ($locked) {$mutex.ReleaseMutex()};$mutex.Dispose()}
        Write-Host "Installed HwpCraft $Version. Local runtime: $Runtime"
    }
    'Start' {
        $exe = Join-Path $Runtime 'hwpcraft.exe'
        if (!(Test-Path -LiteralPath $exe)) {throw 'Run ./HwpCraft.ps1 Build and ./HwpCraft.ps1 Install first.'}
        $options = @{FilePath=$exe;WorkingDirectory=$Project}
        if ($File) {$resolved=(Resolve-Path -LiteralPath $File).Path;$options.ArgumentList=@('"'+$resolved+'"')}
        Start-Process @options | Out-Null
    }
    'Launcher' {& (Join-Path $Workspace 'Craft.ps1') Launcher}
    'Status' {
        [pscustomobject]@{Project=$Project;Version=$Version;Executable=(Join-Path $Runtime 'hwpcraft.exe');Built=(Test-Path (Join-Path $Target 'release/hwpcraft.exe'));Installed=(Test-Path (Join-Path $Runtime 'hwpcraft.exe'))}
    }
}
