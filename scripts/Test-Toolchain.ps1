# Compile only. This script never flashes or opens a serial port.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Enter-DevShell.ps1')
$rfiProjectRoot = Split-Path $PSScriptRoot -Parent
$rfiPreviousTarget = $env:CARGO_TARGET_DIR
try {
    # A short build path avoids ESP-IDF/Windows path length problems.
    $env:CARGO_TARGET_DIR = 'D:\rfi-t'
    Push-Location (Join-Path $rfiProjectRoot 'tools\toolchain-check')
    try {
        cargo build --locked
        if ($LASTEXITCODE -ne 0) { throw "Toolchain build failed with exit code $LASTEXITCODE" }
    } finally { Pop-Location }
} finally { $env:CARGO_TARGET_DIR = $rfiPreviousTarget }
