# Dot-source this script from PowerShell: . .\scripts\Enter-DevShell.ps1
$ErrorActionPreference = 'Stop'
$rfiPythonScripts = Join-Path $env:LOCALAPPDATA 'round-flight-tools\python-env\Scripts'
$rfiBasePython = Join-Path $env:USERPROFILE '.platformio\python3'
$rfiEspExport = Join-Path $env:USERPROFILE 'export-esp.ps1'
if (-not (Test-Path -LiteralPath (Join-Path $rfiPythonScripts 'python.exe'))) {
    throw "Missing project Python environment: $rfiPythonScripts"
}
if (-not (Test-Path -LiteralPath $rfiEspExport)) {
    throw 'Missing Espressif toolchain environment. Run espup install --targets esp32s3 --std.'
}
. $rfiEspExport
# ESP-IDF must create its own Python environment from a base interpreter.
# Keep it ahead of the CMake/Ninja environment's Scripts directory.
if (-not (Test-Path -LiteralPath (Join-Path $rfiBasePython 'python.exe'))) {
    throw "Missing base Python interpreter: $rfiBasePython"
}
$env:PATH = "$rfiBasePython;$rfiPythonScripts;" + $env:PATH
$env:PYTHON = Join-Path $rfiBasePython 'python.exe'
Write-Host 'ESP32-S3 development environment loaded.'
