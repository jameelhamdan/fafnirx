# Builds dist\fafnirx-setup-<version>.exe. The version comes from Cargo.toml, so the installer, its
# file name and Add/Remove Programs all show the same number as `fafnirx --version`.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $version) { throw 'version not found in Cargo.toml' }

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\target" }
Push-Location $root
try { cargo build --release --bin fafnirx; if ($LASTEXITCODE) { throw 'cargo build failed' } } finally { Pop-Location }
$exe = "$target\release\fafnirx.exe"

$built = (& $exe --version) -replace '^fafnirx\s+', ''
if ($built -ne $version) { throw "binary reports $built, Cargo.toml says $version" }

$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
& $iscc "/DAppVersion=$version" "/DExePath=$exe" "$PSScriptRoot\fafnirx.iss"
if ($LASTEXITCODE) { throw 'ISCC failed' }
Write-Host "dist\fafnirx-setup-$version.exe"
