param(
    [switch]$FullRes,
    [int]$TargetWidth = 4096,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

New-Item -ItemType Directory -Force -Path $textureDir | Out-Null

# Always writes lossless PNG — every destination in this script is .png.
function Save-Resized {
    param(
        [string]$SourcePath,
        [string]$DestPath,
        [int]$MaxWidth,
        # Optional exact height. Sources whose aspect is not exactly 2:1 need to
        # be squared up rather than merely width-scaled — see the Vesta note below.
        [int]$ExactHeight = 0
    )
    $bmp = $null
    $resized = $null
    try {
        $bmp = [System.Drawing.Bitmap]::FromFile($SourcePath)
        $w = $bmp.Width
        $h = $bmp.Height
        if ($ExactHeight -gt 0 -and $MaxWidth -gt 0) {
            $w = $MaxWidth
            $h = $ExactHeight
        } elseif ($MaxWidth -gt 0 -and $bmp.Width -gt $MaxWidth) {
            $w = $MaxWidth
            $h = [int][Math]::Round($bmp.Height * ($MaxWidth / $bmp.Width))
        }
        # Draw onto an explicit 8-bit RGBA surface, mirroring the shell port's
        # PNG32: prefix. GDI+ otherwise carries a single-channel source (Ceres,
        # the greyscale Europa/Callisto mosaics, Pluto, Charon) straight through
        # as an indexed PNG — a different pixel format than what
        # compress_textures.* hands the BC7/ASTC encoders.
        $resized = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($resized)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.DrawImage($bmp, 0, 0, $w, $h)
        $g.Dispose()
        $resized.Save($DestPath, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        if ($null -ne $resized) { $resized.Dispose() }
        if ($null -ne $bmp) { $bmp.Dispose() }
    }
}

function Get-And-Convert {
    param(
        [string]$Url,
        [string]$SrcExt,
        [string]$DestPath,
        [int]$ExactHeight = 0
    )
    if ((Test-Path $DestPath) -and -not $Force) {
        Write-Host "Skipping $(Split-Path $DestPath -Leaf) (already present, use -Force to re-download)"
        return
    }
    $tmp = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "minor-body-$([guid]::NewGuid()).$SrcExt")
    try {
        Write-Host "Downloading $Url ..."
        Invoke-WebRequest -Uri $Url -OutFile $tmp
        # Every destination is .png, so even the fast-mode browse renderings —
        # already JPEG at ~1024 wide — are re-encoded rather than copied: a .png
        # must never end up holding JPEG bytes.
        Save-Resized -SourcePath $tmp -DestPath $DestPath -MaxWidth $TargetWidth -ExactHeight $ExactHeight
    } finally {
        if (Test-Path $tmp) { Remove-Item $tmp -Force }
    }
}

# Ceres comes from the USGS Astrogeology mosaic archive (planetarymaps.usgs.gov,
# served from the asc-pds-services S3 bucket): the Dawn FC clear-filter global
# mosaic at 20 ppd, 7383x3691.
$ceresUrl = 'https://asc-pds-services.s3.us-west-2.amazonaws.com/mosaic/Ceres_Dawn_FC_DLR_global_20ppd_Oct2015.tif'

# Vesta's truecolor mosaic exists only on DLR's Dawn GIS server, which is alive
# and serving (verified 2026-07-31 — the note previously here, claiming it had
# stopped responding and that vesta.png was an irreplaceable copy, was wrong).
# Two products are published, and only one is usable here:
#
#   Vesta_true_color_HAMO-1-2_global.png   26704x13080   equirectangular
#   Vesta_true_color_HAMO-1-2.png           2405x1202    MOLLWEIDE
#
# The compact one is an equal-area ellipse with black corners. Mapped onto a UV
# sphere it crushes the terrain into a band and smears the projection's dead
# corners across the globe — that is exactly how the broken map replaced in PR
# #65 got committed. So Vesta is fetched only in -FullRes mode, from the global
# product; fast mode leaves the committed vesta.png alone.
#
# That mosaic is 26704x13080 (~2.042:1), not 2:1, so it is squared up to an
# exact 4096x2048 rather than merely width-scaled: the equirectangular mapping
# assumes 2:1, and 4096x2006 would leave a height that is not 4-aligned, which
# compress_textures would then shrink to 2004 and drift further. Same
# normalisation the committed map got in PR #65.
$vestaGlobalUrl ='https://dawngis.dlr.de/data/Vesta/mosaics/HAMO/truecolor/Vesta_true_color_HAMO-1-2_global.png'

# Galilean moons, USGS Astrogeology global mosaics. Io and Ganymede have
# published colour-merge products; Europa and Callisto exist only as greyscale
# mosaics there, so those two render monochrome (no colour equirectangular map
# of them is published by USGS).

if ($FullRes) {
    Write-Host 'Downloading FULL-RES science mosaics (large files)...'
    Get-And-Convert -Url $ceresUrl -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ceres.png')
    Get-And-Convert -Url $vestaGlobalUrl -SrcExt 'png' -DestPath (Join-Path $textureDir 'vesta.png') -ExactHeight 2048
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Pluto_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'pluto.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Charon_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'charon.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Io_Galileo_SSI_Global_Mosaic_ClrMerge_1km.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'io.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'europa.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1435m.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ganymede.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_GalileoSSI_global_mosaic_1km.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'callisto.png')
} else {
    Write-Host 'Downloading compact science textures (fast mode)...'
    Get-And-Convert -Url $ceresUrl -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ceres.png')
    Write-Host 'Skipping vesta.png (only the Mollweide-projected preview is published at'
    Write-Host '  this size; re-run with -FullRes for the equirectangular mosaic, or restore'
    Write-Host '  the committed map with `git restore assets/textures/vesta.png`)'
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/a5f1b7f4-9822-4697-a201-e23ef4bd3e16/resource/96be2aa1-f384-4a9f-9458-a8431a0e7956/download/pluto_newhorizons_global_mosaic_300m_jul2017_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'pluto.png')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/93827f6c-8feb-42b6-98e6-b0ce57c7d2c8/resource/1abf318c-3290-4aa0-932e-a34f32d7f6ad/download/charon_newhorizons_global_mosaic_300m_jul2017_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'charon.png')
    # 1024-wide browse renderings of the same USGS mosaics used above.
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/0fc15885-24ee-4d9d-9666-11de0667c10c/resource/73d4c1f7-8c07-4b28-90ea-f47f7531c5ca/download/full.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'io.png')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/4080036f-afc5-422e-abe9-1c0c8e4f98ea/resource/3647e7b3-425e-4dcf-951b-cc4a22fb0129/download/europa_voyager_galileossi_global_mosaic_500m_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'europa.png')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/e1422336-3291-4b65-b903-c942d53de073/resource/eb32abd7-fee2-47d1-9f96-9d7d8824cc3a/download/ganymede_voyager_galileossi_global_clrmosaic_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'ganymede.png')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/a80abd68-7ed9-440e-829a-76376779164f/resource/ac628525-cb1c-4742-928b-5a0a60f372cd/download/callisto_voyager_galileossi_global_mosaic_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'callisto.png')
}
# List what is actually on disk rather than what the script hoped to write —
# fast mode deliberately skips Vesta, so a fixed list would claim a file that
# may not be there.
Write-Host 'Science textures present:'
foreach ($name in 'ceres.png', 'vesta.png', 'pluto.png', 'charon.png', 'io.png', 'europa.png', 'ganymede.png', 'callisto.png') {
    $path = Join-Path $textureDir $name
    if (Test-Path $path) {
        Write-Host "  $path"
    } else {
        Write-Host "  $path (missing)"
    }
}
