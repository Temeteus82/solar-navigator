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

# Saves in the format implied by $DestPath's extension. PNG (lossless) is the
# preferred output; JPEG remains only for the legacy .jpg destinations.
function Save-Resized {
    param(
        [string]$SourcePath,
        [string]$DestPath,
        [int]$MaxWidth
    )
    $format = if ([IO.Path]::GetExtension($DestPath) -eq '.png') {
        [System.Drawing.Imaging.ImageFormat]::Png
    } else {
        [System.Drawing.Imaging.ImageFormat]::Jpeg
    }
    $bmp = $null
    $resized = $null
    try {
        $bmp = [System.Drawing.Bitmap]::FromFile($SourcePath)
        if ($MaxWidth -gt 0 -and $bmp.Width -gt $MaxWidth) {
            $newH = [int][Math]::Round($bmp.Height * ($MaxWidth / $bmp.Width))
            $resized = New-Object System.Drawing.Bitmap $MaxWidth, $newH
            $g = [System.Drawing.Graphics]::FromImage($resized)
            $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.DrawImage($bmp, 0, 0, $MaxWidth, $newH)
            $g.Dispose()
            $resized.Save($DestPath, $format)
        } else {
            $bmp.Save($DestPath, $format)
        }
    } finally {
        if ($null -ne $resized) { $resized.Dispose() }
        if ($null -ne $bmp) { $bmp.Dispose() }
    }
}

function Get-And-Convert {
    param(
        [string]$Url,
        [string]$SrcExt,
        [string]$DestPath
    )
    if ((Test-Path $DestPath) -and -not $Force) {
        Write-Host "Skipping $(Split-Path $DestPath -Leaf) (already present, use -Force to re-download)"
        return
    }
    $tmp = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "minor-body-$([guid]::NewGuid()).$SrcExt")
    try {
        Write-Host "Downloading $Url ..."
        Invoke-WebRequest -Uri $Url -OutFile $tmp
        if ($SrcExt -eq 'jpg' -or $SrcExt -eq 'jpeg') {
            Copy-Item $tmp $DestPath -Force
        } else {
            Save-Resized -SourcePath $tmp -DestPath $DestPath -MaxWidth $TargetWidth
        }
    } finally {
        if (Test-Path $tmp) { Remove-Item $tmp -Force }
    }
}

# Ceres comes from the USGS Astrogeology mosaic archive (planetarymaps.usgs.gov,
# served from the asc-pds-services S3 bucket): the Dawn FC clear-filter global
# mosaic at 20 ppd, 7383x3691 — the earlier DLR source (dawngis.dlr.de) stopped
# responding (verified 2026-07). Vesta's truecolor mosaic only ever existed on
# that DLR server; the committed assets/textures/vesta.png (recover with
# `git restore`) is the surviving copy, so it is not re-downloadable — the only
# alternatives online are grayscale or shaded-relief maps.
$ceresUrl = 'https://asc-pds-services.s3.us-west-2.amazonaws.com/mosaic/Ceres_Dawn_FC_DLR_global_20ppd_Oct2015.tif'

# Galilean moons, USGS Astrogeology global mosaics. Io and Ganymede have
# published colour-merge products; Europa and Callisto exist only as greyscale
# mosaics there, so those two render monochrome (no colour equirectangular map
# of them is published by USGS).

if ($FullRes) {
    Write-Host 'Downloading FULL-RES science mosaics (large files)...'
    Get-And-Convert -Url $ceresUrl -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ceres.png')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Pluto_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'pluto.jpg')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Charon_NewHorizons_Global_Mosaic_300m_Jul2017_8bit.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'charon.jpg')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Io_Galileo_SSI_Global_Mosaic_ClrMerge_1km.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'io.jpg')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Europa_Voyager_GalileoSSI_global_mosaic_500m.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'europa.jpg')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Ganymede_Voyager_GalileoSSI_Global_ClrMosaic_1435m.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ganymede.jpg')
    Get-And-Convert -Url 'https://planetarymaps.usgs.gov/mosaic/Callisto_Voyager_GalileoSSI_global_mosaic_1km.tif' -SrcExt 'tif' -DestPath (Join-Path $textureDir 'callisto.jpg')
} else {
    Write-Host 'Downloading compact science textures (fast mode)...'
    Get-And-Convert -Url $ceresUrl -SrcExt 'tif' -DestPath (Join-Path $textureDir 'ceres.png')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/a5f1b7f4-9822-4697-a201-e23ef4bd3e16/resource/96be2aa1-f384-4a9f-9458-a8431a0e7956/download/pluto_newhorizons_global_mosaic_300m_jul2017_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'pluto.jpg')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/93827f6c-8feb-42b6-98e6-b0ce57c7d2c8/resource/1abf318c-3290-4aa0-932e-a34f32d7f6ad/download/charon_newhorizons_global_mosaic_300m_jul2017_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'charon.jpg')
    # 1024-wide browse renderings of the same USGS mosaics used above.
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/0fc15885-24ee-4d9d-9666-11de0667c10c/resource/73d4c1f7-8c07-4b28-90ea-f47f7531c5ca/download/full.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'io.jpg')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/4080036f-afc5-422e-abe9-1c0c8e4f98ea/resource/3647e7b3-425e-4dcf-951b-cc4a22fb0129/download/europa_voyager_galileossi_global_mosaic_500m_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'europa.jpg')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/e1422336-3291-4b65-b903-c942d53de073/resource/eb32abd7-fee2-47d1-9f96-9d7d8824cc3a/download/ganymede_voyager_galileossi_global_clrmosaic_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'ganymede.jpg')
    Get-And-Convert -Url 'https://astrogeology.usgs.gov/ckan/dataset/a80abd68-7ed9-440e-829a-76376779164f/resource/ac628525-cb1c-4742-928b-5a0a60f372cd/download/callisto_voyager_galileossi_global_mosaic_1024.jpg' -SrcExt 'jpg' -DestPath (Join-Path $textureDir 'callisto.jpg')
}
Write-Host 'Note: vesta.png (DLR truecolor) is git-tracked and not re-downloadable; use `git restore assets/textures/vesta.png` if missing.'

Write-Host 'Science textures saved:'
Write-Host "  $textureDir/ceres.png"
Write-Host "  $textureDir/vesta.png"
Write-Host "  $textureDir/pluto.jpg"
Write-Host "  $textureDir/charon.jpg"
Write-Host "  $textureDir/io.jpg"
Write-Host "  $textureDir/europa.jpg"
Write-Host "  $textureDir/ganymede.jpg"
Write-Host "  $textureDir/callisto.jpg"
