param([switch]$Writeback)
$ErrorActionPreference = 'Stop'
$rfiExe = Join-Path $PSScriptRoot 'instrument-connector.exe'
$rfiDll = Join-Path $PSScriptRoot 'SimConnect.dll'
$rfiConfig = Join-Path $PSScriptRoot 'device.json'
foreach ($rfiRequired in @($rfiExe, $rfiDll, $rfiConfig)) {
    if (-not (Test-Path -LiteralPath $rfiRequired -PathType Leaf)) {
        throw "Missing $rfiRequired. Extract the entire test package and copy your existing device.json into it."
    }
}
$rfiStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$rfiResults = Join-Path $PSScriptRoot "test-results\run-$rfiStamp"
New-Item -ItemType Directory -Path $rfiResults | Out-Null
$rfiStarted = Get-Date
$rfiRecord = [ordered]@{
    started_at = $rfiStarted.ToString('o')
    executable_sha256 = (Get-FileHash -LiteralPath $rfiExe -Algorithm SHA256).Hash
    writeback = [bool]$Writeback
    exit_code = $null
    exit_code_hex = $null
    launcher_error = $null
    completed_at = $null
}
$rfiRecord | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $rfiResults 'launch.json') -Encoding UTF8
$rfiTranscript = $false
$rfiPushedLocation = $false
try {
    Start-Transcript -LiteralPath (Join-Path $rfiResults 'console-capture.txt') -NoClobber | Out-Null
    $rfiTranscript = $true
    Push-Location -LiteralPath $PSScriptRoot
    $rfiPushedLocation = $true
    $rfiArguments = @('run', '--config', $rfiConfig, '--msfs', '--dll', $rfiDll)
    if ($Writeback) { $rfiArguments += '--writeback' }
    Write-Host "Results will be saved in $rfiResults"
    # Native stderr is evidence, not a PowerShell exception. Let the executable
    # return, then copy LASTEXITCODE before any other native process is invoked.
    $ErrorActionPreference = 'Continue'
    & $rfiExe @rfiArguments 2>&1 | ForEach-Object { $_.ToString() } | Out-Host
    $rfiRecord.exit_code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($null -ne $rfiRecord.exit_code) {
        $rfiRecord.exit_code_hex = '0x{0:X8}' -f ([int64]$rfiRecord.exit_code -band 4294967295L)
    }
} catch {
    $rfiRecord.launcher_error = $_.Exception.Message
} finally {
    $ErrorActionPreference = 'Stop'
    $rfiRecord.completed_at = (Get-Date).ToString('o')
    $rfiRecord | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $rfiResults 'exit-status.json') -Encoding UTF8
    if ($rfiPushedLocation) { Pop-Location }
    if ($rfiTranscript) { Stop-Transcript | Out-Null }
}
Write-Host "Saved exit status and console output to $rfiResults"
Write-Host 'Return test-results, logs, and manifest.json. Do not include executables.'
if ($null -ne $rfiRecord.launcher_error) { throw $rfiRecord.launcher_error }
if ($null -ne $rfiRecord.exit_code) { exit $rfiRecord.exit_code }
