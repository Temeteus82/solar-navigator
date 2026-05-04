param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$spiceDir = Join-Path $projectRoot 'assets/spice'

New-Item -ItemType Directory -Force -Path $spiceDir | Out-Null

$kernels = @(
    @{ Name = 'naif0012.tls'; Url = 'https://naif.jpl.nasa.gov/pub/naif/generic_kernels/lsk/naif0012.tls' }
    @{ Name = 'de440s.bsp';   Url = 'https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/planets/de440s.bsp' }
    @{ Name = 'pck00011.tpc'; Url = 'https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/pck00011.tpc' }
    @{ Name = 'gm_de440.tpc'; Url = 'https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/gm_de440.tpc' }
)

foreach ($k in $kernels) {
    $dest = Join-Path $spiceDir $k.Name
    if ((Test-Path $dest) -and -not $Force) {
        Write-Host "Skipping $($k.Name) (already present, use -Force to re-download)"
        continue
    }
    Write-Host "Downloading $($k.Name)..."
    Invoke-WebRequest -Uri $k.Url -OutFile $dest
}

Write-Host "SPICE kernels downloaded to $spiceDir"
