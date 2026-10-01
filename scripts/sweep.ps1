<#
.SYNOPSIS
    Prune stale Cargo build artefacts with cargo-sweep.

.DESCRIPTION
    PowerShell sibling of sweep.sh, so Windows needs no bash. Install the tool
    once with `cargo install cargo-sweep`.

.EXAMPLE
    ./scripts/sweep.ps1              # remove artefacts older than 7 days (default)
    ./scripts/sweep.ps1 14           # remove artefacts older than 14 days
    ./scripts/sweep.ps1 stamp        # stamp the current build as "in use"
    ./scripts/sweep.ps1 installed    # keep only artefacts built by installed toolchains
    ./scripts/sweep.ps1 all          # remove ALL artefacts (equivalent to cargo clean)
#>
param(
    [Parameter(Position = 0)]
    [string]$Mode = '7'
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
Set-Location $projectRoot

if (-not (Get-Command cargo-sweep -ErrorAction SilentlyContinue)) {
    Write-Host 'cargo-sweep not found. Install it with:'
    Write-Host '  cargo install cargo-sweep'
    exit 1
}

switch ($Mode) {
    'stamp' {
        Write-Host 'Stamping current artefacts as in-use...'
        cargo sweep --stamp
    }
    'installed' {
        # The cleanup that pays after a toolchain upgrade: the day-based modes can
        # never catch a stale generation, because its artefacts are as recently
        # accessed as the current one's. Without rustup on PATH (e.g. a standalone
        # MSI install of Rust), cargo-sweep falls back to the single `rustc` it can
        # find and keeps only that toolchain's output -- correct for a one-toolchain
        # machine, but re-check with `cargo sweep --dry-run --installed` if you have
        # more than one.
        Write-Host 'Removing artefacts from toolchains that are no longer installed...'
        cargo sweep --installed
    }
    'all' {
        Write-Host 'Removing all build artefacts...'
        cargo sweep --time 0
    }
    default {
        if ($Mode -notmatch '^\d+$') {
            throw "Unknown mode '$Mode'. Expected a number of days, 'stamp', 'installed', or 'all'."
        }
        Write-Host "Removing artefacts older than $Mode days..."
        cargo sweep --time $Mode
    }
}

# $ErrorActionPreference does not apply to native exit codes, so mirror `set -e`.
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Stands in for `du -sh target`, which has no native PowerShell equivalent.
if (Test-Path 'target') {
    $bytes = (Get-ChildItem 'target' -Recurse -File -Force -ErrorAction SilentlyContinue |
        Measure-Object -Sum Length).Sum
    # PowerShell's 1GB/1MB are binary multiples, so the GiB/MiB labels are accurate.
    $size = if ($bytes -ge 1GB) { '{0:N1} GiB' -f ($bytes / 1GB) } else { '{0:N1} MiB' -f ($bytes / 1MB) }
    Write-Host "Done. Current target size: $size"
}
