param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

New-Item -ItemType Directory -Force -Path $textureDir | Out-Null

$baseUrl = 'https://www.solarsystemscope.com/textures/download'

# The resolutions below reproduce the maps committed to assets/textures. This
# list previously asked for the '2k_' variants of eight bodies that are
# committed at 4K or 8K, so running the documented setup downgraded them by up
# to 16x in linear resolution.
#
# Solar System Scope's '8k_' prefix is a product name, not a guarantee: 8k_sun,
# 8k_jupiter and 8k_saturn are all served at 4096x2048. Uranus and Neptune have
# no 8k product at all (those names soft-404 into HTML), so they stay on 2k_,
# which is what their committed maps already are.
$textures = @(
    @{ Remote = '8k_sun.jpg';                Local = 'sun.jpg' }             # 4096x2048
    @{ Remote = '8k_mercury.jpg';            Local = 'mercury.jpg' }         # 8192x4096
    @{ Remote = '8k_venus_surface.jpg';      Local = 'venus.jpg' }           # 8192x4096
    @{ Remote = '8k_earth_daymap.jpg';       Local = 'earth.jpg' }           # 8192x4096
    @{ Remote = '8k_moon.jpg';               Local = 'moon.jpg' }            # 8192x4096
    @{ Remote = '8k_mars.jpg';               Local = 'mars.jpg' }            # 8192x4096
    @{ Remote = '8k_jupiter.jpg';            Local = 'jupiter.jpg' }         # 4096x2048
    @{ Remote = '8k_saturn.jpg';             Local = 'saturn.jpg' }          # 4096x2048
    @{ Remote = '8k_saturn_ring_alpha.png';  Local = 'saturn_ring.png' }     # 8192x500
    @{ Remote = '2k_uranus.jpg';             Local = 'uranus.jpg' }          # 2048x1024 (no 8k product)
    @{ Remote = '2k_neptune.jpg';            Local = 'neptune.jpg' }         # 2048x1024 (no 8k product)
    @{ Remote = '8k_stars_milky_way.jpg';    Local = 'milky_way_8k.jpg' }    # 8192x4096
)

# Solar System Scope answers an unknown texture name with HTTP 200 and an HTML
# error page rather than a 404, so a failed request raises nothing and the HTML
# lands in a .jpg. Check the magic bytes that arrived, not the status code.
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
        Invoke-WebRequest -Uri "$baseUrl/$($t.Remote)" -OutFile $tmp
        if (-not (Test-ImageFile -Path $tmp)) {
            throw "Refusing to write $($t.Local): $($t.Remote) did not return an image"
        }
        Move-Item -Path $tmp -Destination $dest -Force
    } finally {
        if (Test-Path $tmp) { Remove-Item $tmp -Force }
    }
}

Write-Host "Planet textures downloaded to $textureDir"
Write-Host "Milky Way texture downloaded to $textureDir/milky_way_8k.jpg"
Write-Host 'Reminder: verify current license/attribution requirements before redistribution.'
