param(
    [switch]$WithSpice
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
Set-Location $projectRoot

$appSlug = 'solar-navigator'
$appDisplayName = 'Solar Navigator'
$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

$distRoot = Join-Path $projectRoot 'dist/windows'
$stageDir = Join-Path $distRoot ("{0}-{1}-win64" -f $appSlug, $version)
$binaryPath = Join-Path $projectRoot "target/release/$appSlug.exe"
$zipPath = Join-Path $distRoot ("{0}-{1}-win64.zip" -f $appSlug, $version)

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

$buildArgs = @('build', '--release')
if (-not $WithSpice) {
    $buildArgs += '--no-default-features'
}

Write-Host "[1/2] Building release binary (cargo $($buildArgs -join ' '))"
& cargo @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw 'cargo build failed'
}

if (-not (Test-Path $binaryPath)) {
    throw "Build did not produce $binaryPath"
}

Write-Host '[2/2] Creating ZIP package'
if (Test-Path $stageDir) {
    Remove-Item -Recurse -Force $stageDir
}
New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
Copy-Item $binaryPath -Destination (Join-Path $stageDir "$appSlug.exe")
Copy-Item (Join-Path $projectRoot 'assets') -Destination (Join-Path $stageDir 'assets') -Recurse

if (Test-Path $zipPath) {
    Remove-Item -Force $zipPath
}
Push-Location $distRoot
Compress-Archive -Path (Split-Path $stageDir -Leaf) -DestinationPath $zipPath -Force
Pop-Location

Write-Host "Windows package available in $distRoot"
