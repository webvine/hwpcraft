# Keep the sibling reference sources (RHWP and the ArtCraft family) current so
# HwpCraft work always reads the latest upstream code. Fast-forward only: a
# sibling with local changes or a diverged branch is reported, never reset.
# Usage: pwsh -NoProfile -File scripts/sync-siblings.ps1 [-Only rhwp,wordcraft]
param([string[]]$Only = @())
$ErrorActionPreference = 'Continue'
$Parent = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$Self = Split-Path (Split-Path $PSScriptRoot -Parent) -Leaf
$rows = @()
foreach ($dir in Get-ChildItem -Path $Parent -Directory) {
    if ($dir.Name -eq $Self -or -not (Test-Path (Join-Path $dir.FullName '.git'))) { continue }
    if ($Only.Count -gt 0 -and $Only -notcontains $dir.Name) { continue }
    $repo = $dir.FullName
    $before = (git -C $repo rev-parse --short HEAD 2>$null)
    if (-not $before) { $rows += [pscustomobject]@{ repo = $dir.Name; before = ''; after = ''; result = 'no commits' }; continue }
    $dirty = (git -C $repo status --porcelain --untracked-files=no 2>$null)
    git -C $repo fetch --quiet --prune 2>$null
    $branch = (git -C $repo rev-parse --abbrev-ref HEAD 2>$null)
    $upstream = (git -C $repo rev-parse --abbrev-ref '@{u}' 2>$null)
    if (-not $upstream) { $rows += [pscustomobject]@{ repo = $dir.Name; before = $before; after = $before; result = "no upstream ($branch)" }; continue }
    $behind = [int](git -C $repo rev-list --count "HEAD..$upstream" 2>$null)
    if ($behind -eq 0) { $rows += [pscustomobject]@{ repo = $dir.Name; before = $before; after = $before; result = 'up to date' }; continue }
    if ($dirty) { $rows += [pscustomobject]@{ repo = $dir.Name; before = $before; after = $before; result = "behind $behind, local changes: not pulled" }; continue }
    git -C $repo merge --ff-only --quiet $upstream 2>$null
    $after = (git -C $repo rev-parse --short HEAD 2>$null)
    $result = if ($after -ne $before) { "pulled $behind commit(s)" } else { "behind $behind, diverged: not pulled" }
    $rows += [pscustomobject]@{ repo = $dir.Name; before = $before; after = $after; result = $result }
}
$rows | Format-Table -AutoSize
$rhwp = Join-Path $Parent 'rhwp/Cargo.toml'
if (Test-Path $rhwp) {
    $v = (Select-String -Path $rhwp -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    "RHWP version: $v (HwpCraft path dependency; re-run cargo check after a pull)"
}
