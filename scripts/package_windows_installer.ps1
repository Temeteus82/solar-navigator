param(
    [string]$NsisPath
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
Set-Location $projectRoot

$appSlug = 'solar-navigator'
$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

$distRoot      = Join-Path $projectRoot 'dist/windows'
$stageDir      = Join-Path $distRoot ("{0}-{1}-installer-stage" -f $appSlug, $version)
$installerPath = Join-Path $distRoot ("{0}-{1}-win64-setup.exe" -f $appSlug, $version)
$nsiScript     = Join-Path $scriptDir 'installer.nsi'
$cargoTarget   = Join-Path $projectRoot "target/release/$appSlug.exe"

# --- Locate makensis ------------------------------------------------------
if (-not $NsisPath) {
    $cmd = Get-Command 'makensis.exe' -ErrorAction SilentlyContinue
    if ($cmd) {
        $NsisPath = $cmd.Source
    } else {
        $candidates = @(
            "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
            "${env:ProgramFiles}\NSIS\makensis.exe"
        )
        $NsisPath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
}
if (-not $NsisPath -or -not (Test-Path $NsisPath)) {
    throw @"
makensis.exe not found.

Install NSIS (https://nsis.sourceforge.io/Download) — for example:
    winget install NSIS.NSIS
or pass the path explicitly:
    .\scripts\package_windows_installer.ps1 -NsisPath 'C:\Program Files (x86)\NSIS\makensis.exe'
"@
}

# --- Sanity-check CSPICE setup --------------------------------------------
$cspiceLib = Join-Path $projectRoot 'vendor/cspice/cspice/lib/cspice.lib'
if (-not (Test-Path $cspiceLib)) {
    throw @"
CSPICE toolkit not set up at $cspiceLib

Run scripts\setup_cspice_windows_x86_64.ps1 first; the installer needs to
build the SPICE-enabled binary as well as the fallback variant.
"@
}

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
if (Test-Path $stageDir) { Remove-Item -Recurse -Force $stageDir }
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

function Invoke-Cargo {
    param([string[]]$CargoArgs)
    Write-Host "    cargo $($CargoArgs -join ' ')"
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo $($CargoArgs -join ' ') failed" }
}

# --- 1. Build fallback binary --------------------------------------------
Write-Host '[1/5] Building fallback (analytic) release binary'
Invoke-Cargo -CargoArgs @('build', '--release', '--no-default-features')
if (-not (Test-Path $cargoTarget)) { throw "Fallback build did not produce $cargoTarget" }
Copy-Item $cargoTarget (Join-Path $stageDir 'solar-navigator-fallback.exe') -Force

# --- 2. Build SPICE binary -----------------------------------------------
Write-Host '[2/5] Building realistic (SPICE) release binary'
Invoke-Cargo -CargoArgs @('build', '--release')
if (-not (Test-Path $cargoTarget)) { throw "SPICE build did not produce $cargoTarget" }
Copy-Item $cargoTarget (Join-Path $stageDir 'solar-navigator-spice.exe') -Force

# --- 3. Stage non-binary payload -----------------------------------------
Write-Host '[3/5] Staging icon, kernels, and texture download scripts'

$iconStage = Join-Path $stageDir 'icon'
Copy-Item (Join-Path $projectRoot 'assets/icon') $iconStage -Recurse

$spiceSource = Join-Path $projectRoot 'assets/spice'
$kernelStage = Join-Path $stageDir 'spice-kernels'
if (Test-Path $spiceSource) {
    Copy-Item $spiceSource $kernelStage -Recurse
} else {
    New-Item -ItemType Directory -Force -Path $kernelStage | Out-Null
    Write-Warning "assets/spice/ not found — installer will offer SPICE mode but ship no kernels. Run scripts\download_spice_kernels.ps1 first."
}

Copy-Item (Join-Path $scriptDir 'download_textures_solar_system_scope.ps1') $stageDir -Force
Copy-Item (Join-Path $scriptDir 'download_textures_minor_bodies_science.ps1') $stageDir -Force

# --- 4. Build installer ---------------------------------------------------
if (Test-Path $installerPath) { Remove-Item -Force $installerPath }

Write-Host "[4/5] Building installer with NSIS ($NsisPath)"
& $NsisPath /V2 `
    "/DAPP_VERSION=$version" `
    "/DSTAGE_DIR=$stageDir" `
    "/DOUTPUT_FILE=$installerPath" `
    $nsiScript
if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
if (-not (Test-Path $installerPath)) { throw "Installer was not produced at $installerPath" }

# --- 5. Done --------------------------------------------------------------
Write-Host '[5/5] Cleaning staging directory'
Remove-Item -Recurse -Force $stageDir

$sizeMb = [math]::Round((Get-Item $installerPath).Length / 1MB, 1)
Write-Host ''
Write-Host "Installer ready: $installerPath ($sizeMb MB)"
