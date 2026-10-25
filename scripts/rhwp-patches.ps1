# Check, apply or revert HwpCraft's local RHWP patches (patches/rhwp/*.patch).
# Usage: pwsh -NoProfile -File scripts/rhwp-patches.ps1 [-Mode Status|Apply|Revert] [-Rhwp <path>]
# Status lists each patch as applied / not applied / conflict. Apply applies the
# missing ones in order and fails on a conflict without touching later patches.
# Revert removes applied patches in reverse order so RHWP can be fast-forwarded.
param(
    [ValidateSet('Status', 'Apply', 'Revert')][string]$Mode = 'Status',
    [string]$Rhwp = (Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'rhwp')
)
$ErrorActionPreference = 'Stop'
$PatchDir = Join-Path (Split-Path $PSScriptRoot -Parent) 'patches/rhwp'
$patches = @(Get-ChildItem -Path $PatchDir -Filter '*.patch' | Sort-Object Name)
if (-not (Test-Path (Join-Path $Rhwp '.git'))) { throw "RHWP checkout not found: $Rhwp" }

function Get-PatchState([string]$patch) {
    git -C $Rhwp apply --check --whitespace=nowarn $patch 2>$null
    if ($LASTEXITCODE -eq 0) { return 'not applied' }
    git -C $Rhwp apply --check --reverse --whitespace=nowarn $patch 2>$null
    if ($LASTEXITCODE -eq 0) { return 'applied' }
    return 'conflict'
}

$failed = $false
$order = if ($Mode -eq 'Revert') { [array]::Reverse($patches); $patches } else { $patches }
foreach ($p in $order) {
    $state = Get-PatchState $p.FullName
    $result = $state
    if ($Mode -eq 'Apply' -and $state -eq 'not applied') {
        git -C $Rhwp apply --whitespace=nowarn $p.FullName
        $result = if ($LASTEXITCODE -eq 0) { 'applied now' } else { 'apply failed' }
    } elseif ($Mode -eq 'Revert' -and $state -eq 'applied') {
        git -C $Rhwp apply --reverse --whitespace=nowarn $p.FullName
        $result = if ($LASTEXITCODE -eq 0) { 'reverted now' } else { 'revert failed' }
    }
    [pscustomobject]@{ patch = $p.Name; result = $result }
    if ($result -in @('conflict', 'apply failed', 'revert failed')) {
        $failed = $true
        if ($Mode -ne 'Status') { break }
    }
}
if ($failed) {
    Write-Warning 'A patch does not match the current RHWP source. Rebase it by hand (see patches/rhwp/README.md).'
    exit 1
}
