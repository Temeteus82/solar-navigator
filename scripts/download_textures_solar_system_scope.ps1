param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

New-Item -ItemType Directory -Force -Path $textureDir | Out-Null

$baseUrl = 'https://www.solarsystemscope.com/textures/download'

# Solar System Scope refuses Invoke-WebRequest outright: it answers 403 with an
# HTML error page, and no user agent string fixes it (a browser one buys a single
# request before the block returns), while the identical fetch through curl
# succeeds every time. So this script shells out to curl.exe the way the .sh port
# does, rather than trying to make Invoke-WebRequest look like a browser.
# curl.exe has shipped in System32 since Windows 10 1803.
$curlCommand = Get-Command curl.exe -ErrorAction SilentlyContinue
if (-not $curlCommand) {
    throw 'curl.exe not found. It ships with Windows 10 1803 and later; install curl or run scripts/download_textures_solar_system_scope.sh instead.'
}
$curl = $curlCommand.Source

# These textures are the app's only source for these bodies — nothing under
# assets/textures/ is stored in git — so each entry asks for the largest product
# that body actually has. An earlier version of this list asked for the '2k_'
# variants of eight bodies published at 4K or 8K, which downgraded them by up to
# 16x in linear resolution.
#
# Solar System Scope's '8k_' prefix is a product name, not a guarantee: 8k_sun,
# 8k_jupiter and 8k_saturn are all served at 4096x2048. Uranus, Neptune and the
# Venus cloud layer have no 8k product at all (those names soft-404 into HTML),
# so they stay at the largest size that exists.
$textures = @(
    @{ Remote = '8k_sun.jpg';                Local = 'sun.png' }             # 4096x2048
    @{ Remote = '8k_mercury.jpg';            Local = 'mercury.png' }         # 8192x4096
    @{ Remote = '8k_venus_surface.jpg';      Local = 'venus.png' }           # 8192x4096
    @{ Remote = '4k_venus_atmosphere.jpg';   Local = 'venus_clouds.png' }    # 4096x2048 (no 8k product)
    @{ Remote = '8k_earth_daymap.jpg';       Local = 'earth.png' }           # 8192x4096
    @{ Remote = '8k_moon.jpg';               Local = 'moon.png' }            # 8192x4096
    @{ Remote = '8k_mars.jpg';               Local = 'mars.png' }            # 8192x4096
    @{ Remote = '8k_jupiter.jpg';            Local = 'jupiter.png' }         # 4096x2048
    @{ Remote = '8k_saturn.jpg';             Local = 'saturn.png' }          # 4096x2048
    @{ Remote = '8k_saturn_ring_alpha.png';  Local = 'saturn_ring.png' }     # 8192x500
    @{ Remote = '2k_uranus.jpg';             Local = 'uranus.png' }          # 2048x1024 (no 8k product)
    @{ Remote = '2k_neptune.jpg';            Local = 'neptune.png' }         # 2048x1024 (no 8k product)
    @{ Remote = '8k_stars_milky_way.jpg';    Local = 'milky_way_8k.png' }    # 8192x4096
)

# The upstream products are JPEG, but every destination is .png, so the download
# is re-encoded rather than copied — a .png must never end up holding JPEG bytes.
function Convert-ToPng {
    param([string]$SourcePath, [string]$DestPath)
    $bmp = [System.Drawing.Bitmap]::FromFile($SourcePath)
    try {
        $w = $bmp.Width; $h = $bmp.Height
        # Pin 8-bit RGBA, matching the PNG32 convention compress_textures expects.
        $out = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        try {
            # An explicit destination Rectangle is required: DrawImageUnscaled
            # honours the source's DPI metadata, and several of these products
            # are tagged 72dpi (mars 5dpi), which would silently scale and crop
            # them instead of copying pixel-for-pixel.
            $out.SetResolution(96, 96)
            $g = [System.Drawing.Graphics]::FromImage($out)
            try {
                $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
                $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
                $g.DrawImage($bmp, (New-Object System.Drawing.Rectangle 0, 0, $w, $h))
            } finally { $g.Dispose() }
            $out.Save($DestPath, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $out.Dispose() }
    } finally { $bmp.Dispose() }
}

# Solar System Scope answers an unknown texture name with HTTP 200 and an HTML
# error page rather than a 404, so a failed request raises nothing and the HTML
# lands in the texture file. Check the magic bytes that arrived, not the status
# code.
function Test-ImageFile {
    param([string]$Path)
    # Read via FileStream rather than Get-Content: -AsByteStream is PowerShell 6+
    # and -Encoding Byte is 5.1-only, so neither spelling works on both.
    $bytes = New-Object byte[] 8
    $stream = [IO.File]::OpenRead($Path)
    try {
        if ($stream.Read($bytes, 0, 8) -lt 8) { return $false }
    } finally {
        $stream.Dispose()
    }
    # JPEG: FF D8 FF   PNG: 89 50 4E 47 0D 0A 1A 0A
    if ($bytes[0] -eq 0xFF -and $bytes[1] -eq 0xD8 -and $bytes[2] -eq 0xFF) { return $true }
    if ($bytes[0] -eq 0x89 -and $bytes[1] -eq 0x50 -and $bytes[2] -eq 0x4E -and $bytes[3] -eq 0x47) { return $true }
    return $false
}

foreach ($t in $textures) {
    $dest = Join-Path $textureDir $t.Local
    if ((Test-Path $dest) -and -not $Force) {
        Write-Host "Skipping $($t.Local) (already present, use -Force to re-download)"
        continue
    }
    Write-Host "Downloading $($t.Local)..."
    # Download to a temp file so a soft-404 never overwrites a good texture.
    $tmp = [IO.Path]::GetTempFileName()
    try {
        & $curl -fL --silent --show-error --retry 3 --retry-delay 1 "$baseUrl/$($t.Remote)" -o $tmp
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to download $($t.Remote) (curl exit code $LASTEXITCODE)"
        }
        if (-not (Test-ImageFile -Path $tmp)) {
            throw "Refusing to write $($t.Local): $($t.Remote) did not return an image"
        }
        if ([IO.Path]::GetExtension($t.Remote) -eq '.png') {
            Move-Item -Path $tmp -Destination $dest -Force
        } else {
            Convert-ToPng -SourcePath $tmp -DestPath $dest
        }
    } finally {
        if (Test-Path $tmp) { Remove-Item $tmp -Force }
    }
}

Write-Host "Planet textures downloaded to $textureDir"
Write-Host "Milky Way texture downloaded to $textureDir/milky_way_8k.png"
Write-Host 'Reminder: verify current license/attribution requirements before redistribution.'
