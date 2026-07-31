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

# naif.jpl.nasa.gov is a single point of failure for every SPICE-mode CI job,
# and a bare fetch turns one transient connection failure into a red build.
# Invoke-WebRequest only grew -MaximumRetryCount in PowerShell 7, and this
# script is documented as runnable from stock Windows PowerShell 5.1, so the
# retry is written out longhand. $ErrorActionPreference = 'Stop' above makes a
# failed request terminating, hence catchable.
$maxAttempts = 5
for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
    try {
        Invoke-WebRequest -Uri $url -OutFile $archivePath
        break
    } catch {
        if ($attempt -eq $maxAttempts) {
            throw
        }
        $delay = 5 * $attempt
        Write-Host "  attempt $attempt of $maxAttempts failed: $($_.Exception.Message)"
        Write-Host "  retrying in $delay s..."
        Start-Sleep -Seconds $delay
    }
}

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
