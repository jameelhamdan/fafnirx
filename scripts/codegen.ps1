# Regenerates crates/db (the typed queries) from queries/*.sql: see scripts/codegen.sh.
#   $env:DATABASE_URL = 'postgresql://postgres:test@localhost:55432'; scripts\codegen.ps1
$ErrorActionPreference = 'Stop'
if (-not $env:DATABASE_URL) { throw 'set DATABASE_URL to a PostgreSQL server, for example postgresql://postgres:test@localhost:55432' }
Push-Location (Split-Path $PSScriptRoot)
$oldTemp, $oldTmp = $env:TEMP, $env:TMP
try {
    # Cornucopia moves its output out of the temp folder, which must be on the same drive as the repository.
    $work = Join-Path (Split-Path (Get-Location)) 'cornucopia-tmp'
    New-Item -ItemType Directory -Force $work | Out-Null
    $env:TEMP = $env:TMP = $work
    $files = Get-ChildItem crates\master\migrations\*.sql | Sort-Object Name | ForEach-Object { $_.FullName }
    cornucopia fresh --url $env:DATABASE_URL @files
    if ($LASTEXITCODE) { throw 'cornucopia failed' }
    # What is committed is formatted like the rest of the workspace, so a regeneration shows only real changes.
    cargo fmt -p fafnirx-db
} finally {
    $env:TEMP, $env:TMP = $oldTemp, $oldTmp
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    Pop-Location
}
