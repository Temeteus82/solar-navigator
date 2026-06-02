<#
.SYNOPSIS
    Encode the downloaded planet/moon textures into GPU block-compressed,
    mipmapped KTX2 using AMD Compressonator.

.DESCRIPTION
    The format is chosen per platform:
      - macOS / Apple Silicon (Metal): ASTC 4x4  (Metal supports ASTC, not BC7)
      - Windows / Linux desktop GPUs:  BC7        (support BC7, not ASTC)

    Each platform only ever holds its own .ktx2 set (textures are generated
    locally, never committed), and the loader is format-blind
    (util::resolve_texture_load_path picks .ktx2 -> .dds -> the .jpg), so the
    right format is simply selected here at encode time. Running this is an
    opt-in optimisation: block compression keeps textures ~4x smaller in VRAM
    than the RGBA8 the JPEGs decode to, and the embedded mip chain removes the
    shimmer you otherwise get on small/distant bodies.

    Requires Compressonator CLI on PATH: https://gpuopen.com/compressonator/

    The 8K Milky Way backdrop is skipped: its pixels are read on the CPU to
    build the environment cubemap, which cannot come from a block-compressed
    image. The unused sun_2k_backup.jpg is skipped too.

    COLOUR NOTE: planet maps are sRGB base-colour textures. After encoding,
    verify in-app that colours look right. If they appear too dark, the output
    was tagged linear instead of sRGB and must be re-encoded with an sRGB-aware
    setting for your Compressonator version.

    Note: exact Compressonator flags (e.g. the ASTC block-rate syntax) can vary
    by version — adjust $destArgs below if your build rejects them.
#>
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

$tool = Get-Command compressonatorcli -ErrorAction SilentlyContinue
if (-not $tool) {
    Write-Error 'compressonatorcli not found on PATH. Install AMD Compressonator: https://gpuopen.com/compressonator/'
}

# Pick the block-compression format supported by this platform's GPU.
# ($IsMacOS only exists in PowerShell 7+; Windows PowerShell 5.1 leaves it
# undefined, so guard the lookup and default to the desktop BC7 path.)
if ((Test-Path variable:IsMacOS) -and $IsMacOS) {
    $destArgs = @('-fd', 'ASTC', '-BlockRate', '4x4')
    $fmtLabel = 'ASTC 4x4'
} else {
    $destArgs = @('-fd', 'BC7')
    $fmtLabel = 'BC7'
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
    Write-Host "Encoding $($src.Name) -> $($src.BaseName).ktx2 ($fmtLabel + mipmaps)..."
    & $tool.Source @destArgs -miplevels 20 $src.FullName $out | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Compressonator failed for $($src.Name) (exit $LASTEXITCODE)"
    }
}

Write-Host "Compressed ($fmtLabel) textures written to $textureDir"
Write-Host 'The app automatically prefers the .ktx2 files over the .jpg originals.'
