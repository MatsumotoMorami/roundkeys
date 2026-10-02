param([switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $cargoPath = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
    if (-not (Test-Path -LiteralPath $cargoPath)) { throw 'Install Rust first: https://rustup.rs/' }
} else { $cargoPath = $cargo.Source }
if (Test-Path -LiteralPath 'C:/msys64/mingw64/bin/gcc.exe') { $env:Path = 'C:/msys64/mingw64/bin;' + $env:Path }
Push-Location $PSScriptRoot
try {
    & $cargoPath fmt --check
    if ($LASTEXITCODE) { throw 'Formatting check failed' }
    & $cargoPath test --locked
    if ($LASTEXITCODE) { throw 'Tests failed' }
    if ($DebugBuild) { & $cargoPath build --locked; $profile='debug' } else { & $cargoPath build --release --locked; $profile='release' }
    if ($LASTEXITCODE) { throw 'Build failed' }
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    Copy-Item -LiteralPath "target/$profile/roundkeys.exe" -Destination dist/roundkeys.exe
    Copy-Item -LiteralPath README.md,LICENSE -Destination dist
} finally { Pop-Location }
