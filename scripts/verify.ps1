param([switch]$Offline, [switch]$Native, [switch]$Frontend)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $cargoOptions = @('--locked')
    if ($Offline) { $cargoOptions += '--offline' }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed.' }
    & cargo clippy @cargoOptions --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    if ($Native) {
        & cargo test @cargoOptions -- --include-ignored --test-threads=1
    } else {
        & cargo test @cargoOptions
    }
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    if ($Frontend) {
        & python tests/frontend.py
        if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed. Install tests/requirements.txt and run python -m playwright install chromium.' }
    }
} finally { Pop-Location }
