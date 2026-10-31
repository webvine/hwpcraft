# Check, apply or revert HwpCraft's local RHWP patches (patches/rhwp/*.patch).
# Usage: pwsh -NoProfile -File scripts/rhwp-patches.ps1 [-Mode Status|Apply|Revert] [-Rhwp <path>]
# Status lists each patch as applied / not applied / conflict. Apply applies the
# missing ones in order and fails on a conflict without touching later patches.
# Revert removes applied patches in reverse order so RHWP can be fast-forwarded.
#
# Patches are a stack: a later patch may change lines an earlier one added (0009
# changes the PDF code 0002 added), so a patch is judged on a scratch copy of the
# files with the later applied patches taken off first, never on its own.
# Line endings follow the checkout: a Windows checkout made with core.autocrlf=true
# has CRLF files while the patches are LF.
param(
    [ValidateSet('Status', 'Apply', 'Revert')][string]$Mode = 'Status',
    [string]$Rhwp = (Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'rhwp')
)
$ErrorActionPreference = 'Stop'
$PatchDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'patches/rhwp'
$patches = @(Get-ChildItem -Path $PatchDir -Filter '*.patch' | Sort-Object Name)
if (-not (Test-Path (Join-Path $Rhwp '.git'))) { throw "RHWP checkout not found: $Rhwp" }

# Files a patch changes, from its `+++ b/<path>` lines.
function Get-PatchFiles([string]$patch) {
    Get-Content -LiteralPath $patch | Where-Object { $_ -like '+++ b/*' } | ForEach-Object { $_.Substring(6) }
}
$files = @($patches | ForEach-Object { Get-PatchFiles $_.FullName } | Sort-Object -Unique)

# CRLF in the checkout means git must convert when it reads and writes files.
$crlf = $false
foreach ($f in $files) {
    $path = Join-Path $Rhwp $f
    if ((Test-Path -LiteralPath $path) -and ([IO.File]::ReadAllText($path)).Contains("`r`n")) { $crlf = $true; break }
}
$autocrlf = if ($crlf) { 'core.autocrlf=true' } else { 'core.autocrlf=false' }

function Test-Patch([string]$dir, [string]$patch, [switch]$Reverse) {
    $arguments = @('-c', $autocrlf, '-C', $dir, 'apply', '--check', '--whitespace=nowarn')
    if ($Reverse) { $arguments += '--reverse' }
    git @arguments $patch 2>$null
    return $LASTEXITCODE -eq 0
}
function Invoke-Patch([string]$dir, [string]$patch, [switch]$Reverse) {
    $arguments = @('-c', $autocrlf, '-C', $dir, 'apply', '--whitespace=nowarn')
    if ($Reverse) { $arguments += '--reverse' }
    git @arguments $patch
    return $LASTEXITCODE -eq 0
}

# Work out each patch's state on a scratch copy of the touched files.
$scratch = Join-Path ([IO.Path]::GetTempPath()) ("hwpcraft-rhwp-patches-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    git -C $scratch init -q
    foreach ($f in $files) {
        $from = Join-Path $Rhwp $f
        if (Test-Path -LiteralPath $from) {
            $to = Join-Path $scratch $f
            New-Item -ItemType Directory -Force -Path (Split-Path $to -Parent) | Out-Null
            Copy-Item -LiteralPath $from -Destination $to
        }
    }
    $state = @{}
    # Take applied patches off from the top of the stack down.
    for ($i = $patches.Count - 1; $i -ge 0; $i--) {
        $p = $patches[$i].FullName
        if (Test-Patch $scratch $p -Reverse) {
            [void](Invoke-Patch $scratch $p -Reverse)
            $state[$p] = 'applied'
        }
    }
    # Put the whole stack back on in order; what does not fit is a conflict.
    $ok = $true
    foreach ($patch in $patches) {
        $p = $patch.FullName
        if ($ok -and (Test-Patch $scratch $p)) {
            [void](Invoke-Patch $scratch $p)
            if (-not $state.ContainsKey($p)) { $state[$p] = 'not applied' }
        } else {
            $state[$p] = 'conflict'
            $ok = $false
        }
    }
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
}

# A patch applied above a missing one is fine when the scratch run above put the
# whole stack back on: the missing patch is independent of it and Apply adds it in
# place (git refuses, and Apply reports 'apply failed', when it does not fit).
$seenMissing = $false
foreach ($patch in $patches) {
    $s = $state[$patch.FullName]
    if ($s -eq 'not applied') { $seenMissing = $true }
    elseif ($s -eq 'applied' -and $seenMissing) { $state[$patch.FullName] = 'applied (above a missing patch)' }
}

$failed = $false
$order = if ($Mode -eq 'Revert') { $patches[($patches.Count - 1)..0] } else { $patches }
foreach ($patch in $order) {
    $p = $patch.FullName
    $result = $state[$p]
    if ($Mode -eq 'Apply' -and $result -eq 'not applied') {
        $result = if (Invoke-Patch $Rhwp $p) { 'applied now' } else { 'apply failed' }
    } elseif ($Mode -eq 'Revert' -and $result -like 'applied*') {
        $result = if (Invoke-Patch $Rhwp $p -Reverse) { 'reverted now' } else { 'revert failed' }
    }
    [pscustomobject]@{ patch = $patch.Name; result = $result }
    if ($result -in @('conflict', 'apply failed', 'revert failed')) {
        $failed = $true
        if ($Mode -ne 'Status') { break }
    }
}
if ($failed) {
    Write-Warning 'A patch does not match the current RHWP source. Rebase it by hand (see patches/rhwp/README.md).'
    exit 1
}
