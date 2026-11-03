param([string]$SdkRoot = 'C:\MSFS SDK', [switch]$IncludeSdkRuntime, [switch]$IncludeFirmware)
$ErrorActionPreference = 'Stop'
$rfiRoot = Split-Path $PSScriptRoot -Parent
$rfiPreviousSdk = $env:MSFS_SDK
try {
    $env:MSFS_SDK = $SdkRoot
    if (-not (Test-Path -LiteralPath (Join-Path $SdkRoot 'SimConnect SDK\include\SimConnect.h'))) { throw 'MSFS 2020 SDK header missing' }
    Push-Location $rfiRoot
    try {
        cargo build -p sim-bridge -p desktop-cli --features sim-bridge/msfs --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Native bridge build failed' }
        $rfiStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
        $rfiPackage = Join-Path $rfiRoot "artifacts\msfs2020-test-$rfiStamp"
        New-Item -ItemType Directory -Path $rfiPackage | Out-Null
        foreach ($rfiSubdir in @('bin','scripts','fixtures','logs')) { New-Item -ItemType Directory -Path (Join-Path $rfiPackage $rfiSubdir) | Out-Null }
        foreach ($rfiExe in @('sim-bridge.exe','desktop-cli.exe')) { Copy-Item -LiteralPath (Join-Path $rfiRoot "target\release\$rfiExe") -Destination (Join-Path $rfiPackage "bin\$rfiExe") }
        foreach ($rfiScript in @('cli.py','sim_bridge.py','start_msfs.py','flash_test_firmware.py','partition_layout.py')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $rfiScript) -Destination (Join-Path $rfiPackage "scripts\$rfiScript") }
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'fixtures\altimeter-lifecycle.txt') -Destination (Join-Path $rfiPackage 'fixtures')
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'docs\MSFS-TWO-WAY-TEST.md') -Destination (Join-Path $rfiPackage 'START-HERE.md')
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'docs\MSFS-TWO-WAY-TEST.md') -Destination (Join-Path $rfiPackage 'TWO-WAY-TEST.md')
        if ($IncludeFirmware) {
            & (Join-Path $PSScriptRoot 'Build-Console.ps1')
            $rfiFirmware = Get-Content -LiteralPath 'D:\rfi-c\console-build-artifacts.json' -Raw | ConvertFrom-Json
            if ($rfiFirmware.watchdog_self_test -or (Get-FileHash -LiteralPath $rfiFirmware.executable).Hash -ne $rfiFirmware.sha256) { throw 'Firmware artifact verification failed' }
            New-Item -ItemType Directory -Path (Join-Path $rfiPackage 'firmware') | Out-Null
            python (Join-Path $PSScriptRoot 'partition_layout.py') --csv $rfiFirmware.partition_csv --binary $rfiFirmware.partition_binary --sdkconfig $rfiFirmware.sdkconfig
            if ($LASTEXITCODE -ne 0) { throw 'Partition verification failed' }
            foreach ($rfiArtifact in @('bootloader','partition_csv','partition_binary')) {
                if ((Get-FileHash $rfiFirmware.$rfiArtifact).Hash -ne $rfiFirmware.($rfiArtifact + '_sha256')) { throw "Changed firmware artifact: $rfiArtifact" }
            }
            # Separate segments leave NVS/PHY untouched; a merged image writes
            # padding over those sectors even if their offsets are unchanged.
            espflash save-image --skip-update-check --chip esp32s3 --flash-size 16mb --partition-table $rfiFirmware.partition_csv $rfiFirmware.executable (Join-Path $rfiPackage 'firmware\application.bin')
            if ($LASTEXITCODE -ne 0) { throw 'Firmware image generation failed' }
            Copy-Item $rfiFirmware.bootloader (Join-Path $rfiPackage 'firmware\bootloader.bin')
            Copy-Item $rfiFirmware.partition_binary (Join-Path $rfiPackage 'firmware\partition-table.bin')
            Copy-Item $rfiFirmware.partition_csv (Join-Path $rfiPackage 'firmware\partitions.csv')
            Copy-Item -LiteralPath $rfiFirmware.executable -Destination (Join-Path $rfiPackage 'firmware\rfi-console.elf')
        }
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'LICENSE') -Destination $rfiPackage
        'pyserial==3.5' | Set-Content -LiteralPath (Join-Path $rfiPackage 'requirements.txt') -Encoding ASCII
        if ($IncludeSdkRuntime) {
            Copy-Item -LiteralPath (Join-Path $SdkRoot 'SimConnect SDK\lib\SimConnect.dll') -Destination $rfiPackage
            'SimConnect.dll is an unmodified Microsoft SDK runtime copied from this development PC for this local test package. The project Apache-2.0 license does not apply to it. Source: MSFS 2020 Core SDK.' | Set-Content -LiteralPath (Join-Path $rfiPackage 'MICROSOFT-RUNTIME.txt') -Encoding UTF8
        }
        python (Join-Path $PSScriptRoot 'snapshot_sources.py') $rfiPackage
        if ($LASTEXITCODE -ne 0) { throw 'Source snapshot failed' }
        $rfiFiles = Get-ChildItem -LiteralPath $rfiPackage -Recurse -File | ForEach-Object {
            [ordered]@{path=$_.FullName.Substring($rfiPackage.Length + 1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
        }
        [ordered]@{adapter_revision='barometer-two-way-v2';purpose='MSFS 2020 two-way bench checkpoint; Just Flight Warrior confirmed, broader aircraft/flight acceptance pending';branch=(git branch --show-current);base_commit=(git rev-parse HEAD);working_tree_modified=[bool](git status --porcelain);sdk_version=(Get-Content -LiteralPath (Join-Path $SdkRoot 'version.txt') -Raw).Trim();includes_local_sdk_runtime=[bool]$IncludeSdkRuntime;includes_firmware=[bool]$IncludeFirmware;files=@($rfiFiles)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $rfiPackage 'manifest.json') -Encoding UTF8
        Compress-Archive -Path (Join-Path $rfiPackage '*') -DestinationPath "$rfiPackage.zip"
        Get-FileHash -LiteralPath "$rfiPackage.zip" -Algorithm SHA256 | Format-List
        Write-Output "Test package: $rfiPackage.zip"
        Write-Output "Expanded folder: $rfiPackage"
    } finally { Pop-Location }
} finally { $env:MSFS_SDK = $rfiPreviousSdk }
