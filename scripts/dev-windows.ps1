$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo fmt --all --check
if ($LASTEXITCODE) { exit $LASTEXITCODE }
cargo test -p meshclip-core --locked
if ($LASTEXITCODE) { exit $LASTEXITCODE }
Set-Location (Join-Path $root "apps/desktop")
npm ci
if ($LASTEXITCODE) { exit $LASTEXITCODE }
npm run tauri dev
