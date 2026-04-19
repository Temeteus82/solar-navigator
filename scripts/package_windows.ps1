param(
    [switch]$WithSpice,
    [switch]$SkipMsi
)

$ErrorActionPreference = 'Stop'

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Resolve-Path (Join-Path $scriptDir '..')
Set-Location $projectRoot

$appSlug = 'solar-navigator'
$appDisplayName = 'Solar Navigator'
$version = (Select-String -Path 'Cargo.toml' -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

$versionParts = ($version -split '[^0-9]+' | Where-Object { $_ -ne '' })
while ($versionParts.Count -lt 3) {
    $versionParts += '0'
}
$msiVersion = "$($versionParts[0]).$($versionParts[1]).$($versionParts[2])"

$distRoot = Join-Path $projectRoot 'dist/windows'
$stageDir = Join-Path $distRoot ("{0}-{1}-win64" -f $appSlug, $version)
$binaryPath = Join-Path $projectRoot "target/release/$appSlug.exe"
$zipPath = Join-Path $distRoot ("{0}-{1}-win64.zip" -f $appSlug, $version)
$msiPath = Join-Path $distRoot ("{0}-{1}-win64.msi" -f $appSlug, $version)

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

$buildArgs = @('build', '--release')
if (-not $WithSpice) {
    $buildArgs += '--no-default-features'
}

Write-Host "[1/3] Building release binary (cargo $($buildArgs -join ' '))"
& cargo @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw 'cargo build failed'
}

if (-not (Test-Path $binaryPath)) {
    throw "Build did not produce $binaryPath"
}

Write-Host '[2/3] Creating ZIP package'
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

Write-Host '[3/3] Creating MSI package (if WiX is available)'
if (-not $SkipMsi -and (Get-Command wix -ErrorAction SilentlyContinue)) {
    $wxsPath = Join-Path $distRoot 'solar-navigator.wxs'

    @"
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package
      Name="$appDisplayName"
      Manufacturer="Solar Navigator Contributors"
      Version="$msiVersion"
      UpgradeCode="{D73C5B70-8DA4-4A15-8E84-6A9A96F04359}"
      Scope="perMachine"
      Language="1033"
      Codepage="1252"
      InstallerVersion="500">
    <SummaryInformation Description="$appDisplayName Installer" />
    <MediaTemplate EmbedCab="yes" />

    <StandardDirectory Id="ProgramFiles64Folder">
      <Directory Id="INSTALLFOLDER" Name="$appDisplayName">
        <Files Include="`$(var.SourceDir)\\**" />
      </Directory>
    </StandardDirectory>

    <Feature Id="MainFeature" Title="$appDisplayName" Level="1">
      <ComponentGroupRef Id="INSTALLFOLDER" />
    </Feature>
  </Package>
</Wix>
"@ | Set-Content -Encoding UTF8 $wxsPath

    if (Test-Path $msiPath) {
        Remove-Item -Force $msiPath
    }

    & wix build -nologo -arch x64 -d "SourceDir=$stageDir" -o $msiPath $wxsPath
    if ($LASTEXITCODE -ne 0) {
        throw 'WiX failed to build MSI package'
    }
}
else {
    Write-Host '  Skipping MSI build: WiX (`wix` command) was not found or --SkipMsi was set.'
}

Write-Host "Windows packages available in $distRoot"
