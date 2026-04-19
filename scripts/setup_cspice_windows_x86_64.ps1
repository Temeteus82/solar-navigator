param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
$vendorDir = Join-Path $projectRoot 'vendor/cspice'
$cspiceDir = Join-Path $vendorDir 'cspice'
$archivePath = Join-Path $vendorDir 'cspice.zip'
$url = 'https://naif.jpl.nasa.gov/pub/naif/toolkit//C/PC_Windows_VisualC_64bit/packages/cspice.zip'

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'Unsupported Windows architecture: 32-bit. Expected x86_64.'
}

$libPath = Join-Path $cspiceDir 'lib/cspice.lib'
if ((Test-Path $libPath) -and -not $Force) {
    Write-Host "CSPICE already installed at $cspiceDir"
    exit 0
}

if (Test-Path $cspiceDir) {
    Remove-Item -Recurse -Force $cspiceDir
}
New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null

Write-Host 'Downloading Windows x86_64 CSPICE toolkit...'
Invoke-WebRequest -Uri $url -OutFile $archivePath

Write-Host 'Extracting CSPICE toolkit...'
Expand-Archive -Path $archivePath -DestinationPath $vendorDir -Force

$headerPath = Join-Path $cspiceDir 'include/SpiceUsr.h'
if (-not (Test-Path $headerPath)) {
    throw "CSPICE install failed: missing header $headerPath"
}

if (-not (Test-Path $libPath)) {
    throw "CSPICE install failed: missing static library $libPath"
}

Write-Host "CSPICE installed at $cspiceDir"
