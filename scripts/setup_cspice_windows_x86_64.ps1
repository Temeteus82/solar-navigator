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

# `cspice-sys` generates its FFI bindings with bindgen, which loads libclang at
# build time. A machine can therefore have a perfectly good CSPICE install here
# and still fail `cargo build --features spice` inside a dependency's build
# script, which is a confusing place to learn about a missing prerequisite.
# Surface it at setup time instead.
#
# This warns rather than throws: installing CSPICE is still worth doing while
# libclang is being installed, the portable build never needs it at all, and
# bindgen's own search covers more locations than are enumerated below.
function Find-LibClang {
    $candidates = @()

    # An explicit LIBCLANG_PATH wins, since that is what bindgen consults first.
    if ($env:LIBCLANG_PATH) {
        $candidates += $env:LIBCLANG_PATH
    }

    foreach ($programFiles in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
        if ($programFiles) {
            $candidates += (Join-Path $programFiles 'LLVM/bin')
        }
    }

    # The "C++ Clang tools for Windows" VS component installs libclang inside the
    # Visual Studio tree rather than under Program Files\LLVM.
    if (${env:ProgramFiles(x86)}) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        if (Test-Path $vswhere) {
            foreach ($vs in (& $vswhere -products * -format value -property installationPath)) {
                if ($vs) {
                    $candidates += (Join-Path $vs 'VC/Tools/Llvm/x64/bin')
                    $candidates += (Join-Path $vs 'VC/Tools/Llvm/bin')
                }
            }
        }
    }

    foreach ($dir in ($env:PATH -split ';')) {
        if ($dir) {
            $candidates += $dir
        }
    }

    foreach ($dir in $candidates) {
        $dll = Join-Path $dir 'libclang.dll'
        if (Test-Path $dll) {
            return $dll
        }
    }

    return $null
}

$libclang = Find-LibClang
if ($libclang) {
    Write-Host "libclang found at $libclang"
} else {
    Write-Warning @'
libclang was not found, so the SPICE build will fail in the cspice-sys build
script ("Unable to find libclang"). Install it with one of:
    winget install LLVM.LLVM
    Visual Studio Installer -> C++ Clang tools for Windows
Then re-open the shell, or set LIBCLANG_PATH to the directory holding
libclang.dll. The portable build (--no-default-features) does not need it.
'@
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
