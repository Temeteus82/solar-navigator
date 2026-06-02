<#
.SYNOPSIS
    Encode the downloaded planet/moon textures into GPU block-compressed KTX2
    (BC7 + mipmaps) using AMD Compressonator.

.DESCRIPTION
    The app prefers a same-stem .ktx2 (or .dds) over the plain .jpg download at
    load time (see util::resolve_texture_load_path), so running this is purely
    an opt-in optimisation: it does not change which bodies render.

    BC7 keeps texture data block-compressed in VRAM (~4x smaller than the
    RGBA8 the JPEGs decode to) and the embedded mip chain removes the shimmer
    you otherwise get from un-mipmapped maps on small/distant bodies.

    Requires Compressonator CLI on PATH: https://gpuopen.com/compressonator/

    The 8K Milky Way backdrop is intentionally skipped: its pixels are read on
    the CPU to build the environment cubemap, which cannot be done from a
    block-compressed image. The unused sun_2k_backup.jpg is skipped too.

    COLOUR NOTE: planet maps are sRGB base-colour textures. After encoding,
    verify in-app that colours look right. If they appear too dark, the KTX2
    was tagged linear instead of sRGB and must be re-encoded with an sRGB-aware
    setting for your Compressonator version.
#>
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

# macOS is supported on Apple Silicon only, and Apple Silicon GPUs (Metal)
# cannot load BC7/BCn textures. Skip compression there — the app falls back to
# the .jpg textures automatically. ($IsMacOS only exists in PowerShell 7+;
# Windows PowerShell 5.1 leaves it undefined, so guard the lookup.)
if ((Test-Path variable:IsMacOS) -and $IsMacOS) {
    Write-Host 'Skipping BC7 compression on macOS: Apple Silicon GPUs do not support BC7.'
    Write-Host 'The app loads the .jpg textures directly on macOS.'
    exit 0
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

$tool = Get-Command compressonatorcli -ErrorAction SilentlyContinue
if (-not $tool) {
    Write-Error 'compressonatorcli not found on PATH. Install AMD Compressonator: https://gpuopen.com/compressonator/'
}

# Textures that must NOT be compressed: the CPU-read backdrop and the unused
# lower-resolution sun backup.
$skip = @('milky_way_8k.jpg', 'sun_2k_backup.jpg')

$sources = Get-ChildItem -Path $textureDir -Filter '*.jpg' |
    Where-Object { $skip -notcontains $_.Name }

foreach ($src in $sources) {
    $out = Join-Path $textureDir ($src.BaseName + '.ktx2')
    if ((Test-Path $out) -and -not $Force) {
        Write-Host "Skipping $($src.BaseName).ktx2 (already present, use -Force to re-encode)"
        continue
    }
    Write-Host "Encoding $($src.Name) -> $($src.BaseName).ktx2 (BC7 + mipmaps)..."
    & $tool.Source -fd BC7 -miplevels 20 $src.FullName $out | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Compressonator failed for $($src.Name) (exit $LASTEXITCODE)"
    }
}

Write-Host "Compressed textures written to $textureDir"
Write-Host 'The app automatically prefers the .ktx2 files over the .jpg originals.'
