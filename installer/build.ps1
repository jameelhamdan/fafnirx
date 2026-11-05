# Builds dist\fafnirx-setup-<version>.exe around an already built client:
#   cargo build --profile client --locked -p fafnirx; installer\build.ps1
# The version comes from Cargo.toml, so the installer, its file name and Add/Remove Programs show the number `fafnirx --version` does.
param([string]$Exe = "$(Split-Path $PSScriptRoot)\target\client\fafnirx.exe")
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw 'version not found in Cargo.toml' }
$built = (& $Exe --version) -replace '^fafnirx\s+', ''
if ($built -ne $version) { throw "binary reports $built, Cargo.toml says $version" }

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
& $iscc "/DAppVersion=$version" "/DExePath=$((Resolve-Path $Exe).Path)" "$PSScriptRoot\fafnirx.iss"
if ($LASTEXITCODE) { throw 'ISCC failed' }
Write-Host "dist\fafnirx-setup-$version.exe"
