# Builds the Windows release artifacts into dist/:
#   Keyless-<version>-x64-portable.zip, Keyless-Setup-<version>-x64.exe, SHA256SUMS.txt
# Used by the release workflow and for local testing. Needs Inno Setup 6 (iscc) unless -SkipInstaller.
param([switch]$SkipInstaller)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$meta = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$version = ($meta.packages | Where-Object name -eq 'keyless').version
Write-Host "Keyless $version"

$env:KEYLESS_REQUIRE_RESOURCES = '1'
cargo build --release --locked -p keyless
if ($LASTEXITCODE) { throw 'cargo build failed' }

$stage = Join-Path $root 'target\package'
$dist = Join-Path $root 'dist'
Remove-Item -Recurse -Force $stage, $dist -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage, $dist | Out-Null
Copy-Item target\release\keyless.exe, LICENSE, README.md $stage
Copy-Item packaging\licenses\*, assets\fonts\OFL-*.txt $stage

if (Get-Command cargo-about -ErrorAction SilentlyContinue) {
    cargo about generate --fail packaging\about.hbs -o "$stage\THIRD_PARTY_LICENSES.html"
    if ($LASTEXITCODE) { throw 'cargo about failed' }
} else {
    Write-Warning 'cargo-about not installed: skipping THIRD_PARTY_LICENSES.html (cargo install cargo-about --locked)'
}

Compress-Archive -Path "$stage\*" -DestinationPath "$dist\Keyless-$version-x64-portable.zip"

if (-not $SkipInstaller) {
    $iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        # Per-machine install, then per-user install (winget --scope user).
        $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") |
            Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc -or -not (Test-Path $iscc)) { throw 'Inno Setup 6 not found. Install it or pass -SkipInstaller.' }
    & $iscc /Qp "/DAppVersion=$version" "/DSourceDir=$stage" "/DOutputDir=$dist" packaging\windows\keyless.iss
    if ($LASTEXITCODE) { throw 'Installer build failed' }
}

Get-ChildItem $dist -File | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
} | Set-Content "$dist\SHA256SUMS.txt" -Encoding ascii
Get-ChildItem $dist | Select-Object Name, Length
