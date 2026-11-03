param([string]$Port = 'COM5', [switch]$WatchdogSelfTest)
$ErrorActionPreference = 'Stop'
$rfiRoot = Split-Path $PSScriptRoot -Parent
& (Join-Path $PSScriptRoot 'Build-Console.ps1') -WatchdogSelfTest:$WatchdogSelfTest
$rfiDevice = @(Get-PnpDevice -PresentOnly -Class Ports | Where-Object {
    $_.InstanceId -eq 'USB\VID_1A86&PID_55D3\58FA088072' -and
    $_.FriendlyName.EndsWith("($Port)") -and $_.Status -eq 'OK'
})
if ($rfiDevice.Count -ne 1) { throw "Verified Waveshare UART adapter is not present on $Port" }
$rfiBackup = Join-Path $rfiRoot 'backups\factory-20261031-213635\factory-flash-16MiB.bin'
$rfiExpectedHash = '3CBEA10793EB9CEA69E0A2B9BB91DF25D1FB6B8B4622F49CD8DAE071A77FF2BA'
if ((Get-FileHash -LiteralPath $rfiBackup -Algorithm SHA256).Hash -ne $rfiExpectedHash) {
    throw 'Factory backup checksum does not match the verified original'
}
$rfiArtifacts = Get-Content -LiteralPath 'D:\rfi-c\console-build-artifacts.json' -Raw | ConvertFrom-Json
if ((Get-FileHash -LiteralPath $rfiArtifacts.executable).Hash -ne $rfiArtifacts.sha256) { throw 'Executable changed after verification' }
foreach ($rfiArtifact in @('bootloader','partition_csv','partition_binary')) {
    if ((Get-FileHash -LiteralPath $rfiArtifacts.$rfiArtifact).Hash -ne $rfiArtifacts.($rfiArtifact + '_sha256')) { throw "Changed build artifact: $rfiArtifact" }
}
python (Join-Path $PSScriptRoot 'partition_layout.py') --csv $rfiArtifacts.partition_csv --binary $rfiArtifacts.partition_binary --sdkconfig $rfiArtifacts.sdkconfig
if ($LASTEXITCODE -ne 0) { throw 'Partition table verification failed' }
espflash flash --chip esp32s3 --port $Port --baud 460800 --non-interactive --skip-update-check --flash-size 16mb --bootloader $rfiArtifacts.bootloader --partition-table $rfiArtifacts.partition_csv $rfiArtifacts.executable
if ($LASTEXITCODE -ne 0) { throw "Flashing failed: $LASTEXITCODE" }
Write-Output 'Application CLI: 921600/8N1, no flow control. Wait for READY before commands; early boot logs use 115200.'
