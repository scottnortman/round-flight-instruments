param([string]$Port = 'COM5')
$ErrorActionPreference = 'Stop'
$rfiRoot = Split-Path $PSScriptRoot -Parent
try {
    & (Join-Path $PSScriptRoot 'Flash-Console.ps1') -Port $Port -WatchdogSelfTest
    & D:\rfi-e\python_env\idf5.5_py3.11_env\Scripts\python.exe (Join-Path $PSScriptRoot 'capture-watchdog.py') --port $Port --log (Join-Path $rfiRoot 'verification\watchdog-uart.log')
    if ($LASTEXITCODE -ne 0) { throw 'Hardware watchdog acceptance failed' }
} finally {
    # Always restore normal firmware, including after a failed capture/test.
    & (Join-Path $PSScriptRoot 'Flash-Console.ps1') -Port $Port
}
