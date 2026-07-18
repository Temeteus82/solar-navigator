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

# BC7 and ASTC 4x4 both tile the image in 4x4 blocks, so the GPU rejects any
# texture whose width or height is not a multiple of 4 (wgpu panics with
# "Height N is not a multiple of Bc7RgbaUnormSrgb's block height (4)"). A few
# source maps ship with odd dimensions (e.g. Vesta's 2405x1202). The
# Compressonator CLI has no resize flags (verified against 4.5.52), so those
# sources are pre-shrunk here to the nearest multiple of 4 as a temp PNG
# (lossless — avoids a second JPEG generation loss) and that is encoded
# instead. Returns the path to feed Compressonator: the original when already
# aligned or unreadable, otherwise the temp PNG (caller deletes it).
function Get-AlignedSource([string]$path) {
    try {
        Add-Type -AssemblyName System.Drawing -ErrorAction Stop
        $img = [System.Drawing.Image]::FromFile($path)
    } catch {
        Write-Warning "  Could not read dimensions of $(Split-Path $path -Leaf); encoding without 4x4 block-alignment resize."
        return $path
    }
    try {
        $w = $img.Width; $h = $img.Height
        $tw = $w - ($w % 4)
        $th = $h - ($h % 4)
        if ($tw -eq $w -and $th -eq $h) { return $path }
        Write-Host "  $(Split-Path $path -Leaf) is ${w}x${h}; resizing to ${tw}x${th} for 4x4 block alignment"
        $tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetFileNameWithoutExtension($path) + '_4x4aligned.png')
        $dst = New-Object System.Drawing.Bitmap $tw, $th
        try {
            $g = [System.Drawing.Graphics]::FromImage($dst)
            try {
                $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $g.DrawImage($img, 0, 0, $tw, $th)
            } finally { $g.Dispose() }
            $dst.Save($tmp, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $dst.Dispose() }
        return $tmp
    } finally { $img.Dispose() }
}

# Textures that must NOT be compressed: the CPU-read backdrop, the unused
# lower-resolution sun backup, and the ring texture (setup.rs loads
# saturn_ring.png directly, not via resolve_texture_load_path, so a .ktx2
# would never be picked up).
$skip = @('milky_way_8k.jpg', 'sun_2k_backup.jpg', 'saturn_ring.png')

$sources = Get-ChildItem -Path (Join-Path $textureDir '*') -Include '*.jpg', '*.png' |
    Where-Object { $skip -notcontains $_.Name }

foreach ($src in $sources) {
    $out = Join-Path $textureDir ($src.BaseName + '.ktx2')
    if ((Test-Path $out) -and -not $Force) {
        Write-Host "Skipping $($src.BaseName).ktx2 (already present, use -Force to re-encode)"
        continue
    }
    $encodeSrc = Get-AlignedSource $src.FullName
    Write-Host "Encoding $($src.Name) -> $($src.BaseName).ktx2 ($fmtLabel + mipmaps)..."
    & $tool.Source @destArgs -miplevels 20 $encodeSrc $out | Out-Null
    $exitCode = $LASTEXITCODE
    if ($encodeSrc -ne $src.FullName) {
        Remove-Item $encodeSrc -ErrorAction SilentlyContinue
    }
    if ($exitCode -ne 0) {
        Write-Error "Compressonator failed for $($src.Name) (exit $exitCode)"
    }
}

Write-Host "Compressed ($fmtLabel) textures written to $textureDir"
Write-Host 'The app automatically prefers the .ktx2 files over the .jpg originals.'
