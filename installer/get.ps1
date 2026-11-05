# Installs the fafnirx client on Windows in one command: downloads the program, checks its sha256, and runs `fafnirx install`.
# No administrator rights are needed: the client belongs to your user.
#
#   irm https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.ps1 -OutFile get.ps1
#   .\get.ps1 -Server nas.lan -Folder $HOME\Documents
#
# The device token is asked for, or taken from $env:FAFNIRX_TOKEN. -Version 4.0.4 installs that version instead of the latest.
param(
    [string]$Server,
    [string[]]$Folder = @(),
    [string]$Version = $env:FAFNIRX_VERSION,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'
$repo = 'jameelhamdan/fafnirx'
if (-not $Version) { $Version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name.TrimStart('v') }

$name = "fafnirx-$Version-windows-x86_64.exe"
$url = "https://github.com/$repo/releases/download/v$Version"
$work = Join-Path ([IO.Path]::GetTempPath()) "fafnirx-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $exe = Join-Path $work 'fafnirx.exe'
    Write-Host "downloading $name"
    Invoke-WebRequest "$url/$name" -OutFile $exe
    $expected = ((Invoke-RestMethod "$url/$name.sha256") -split '\s+')[0]
    if ((Get-FileHash $exe -Algorithm SHA256).Hash -ne $expected) { throw 'the download does not match its sha256; nothing was installed' }

    $install = @('install')
    if ($Server) { $install += '--server', $Server }
    foreach ($f in $Folder) { $install += '--folder', $f }
    if ($DryRun) { $install += '--dry-run' }
    & $exe @install
    if ($LASTEXITCODE) { throw "fafnirx install failed ($LASTEXITCODE)" }
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
