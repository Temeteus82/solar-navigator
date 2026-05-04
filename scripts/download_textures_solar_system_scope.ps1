param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$textureDir = Join-Path $projectRoot 'assets/textures'

New-Item -ItemType Directory -Force -Path $textureDir | Out-Null

$baseUrl = 'https://www.solarsystemscope.com/textures/download'

# Solar System Scope's 8k_sun is currently 4096x2048 and gives better detail
# than the previous 2k map used for the app's emissive sun.
$textures = @(
    @{ Remote = '8k_sun.jpg';                Local = 'sun.jpg' }
    @{ Remote = '2k_mercury.jpg';            Local = 'mercury.jpg' }
    @{ Remote = '2k_venus_surface.jpg';      Local = 'venus.jpg' }
    @{ Remote = '2k_earth_daymap.jpg';       Local = 'earth.jpg' }
    @{ Remote = '2k_moon.jpg';               Local = 'moon.jpg' }
    @{ Remote = '2k_mars.jpg';               Local = 'mars.jpg' }
    @{ Remote = '2k_jupiter.jpg';            Local = 'jupiter.jpg' }
    @{ Remote = '2k_saturn.jpg';             Local = 'saturn.jpg' }
    @{ Remote = '2k_saturn_ring_alpha.png';  Local = 'saturn_ring.png' }
    @{ Remote = '2k_uranus.jpg';             Local = 'uranus.jpg' }
    @{ Remote = '2k_neptune.jpg';            Local = 'neptune.jpg' }
    @{ Remote = '8k_stars_milky_way.jpg';    Local = 'milky_way_8k.jpg' }
)

foreach ($t in $textures) {
    $dest = Join-Path $textureDir $t.Local
    if ((Test-Path $dest) -and -not $Force) {
        Write-Host "Skipping $($t.Local) (already present, use -Force to re-download)"
        continue
    }
    Write-Host "Downloading $($t.Local)..."
    Invoke-WebRequest -Uri "$baseUrl/$($t.Remote)" -OutFile $dest
}

Write-Host "Planet textures downloaded to $textureDir"
Write-Host "Milky Way texture downloaded to $textureDir/milky_way_8k.jpg"
Write-Host 'Reminder: verify current license/attribution requirements before redistribution.'
