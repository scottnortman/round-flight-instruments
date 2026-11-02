param([string]$SdkRoot = 'C:\MSFS SDK', [switch]$IncludeSdkRuntime)
$ErrorActionPreference = 'Stop'
$rfiRoot = Split-Path $PSScriptRoot -Parent
$rfiPreviousSdk = $env:MSFS_SDK
try {
    $env:MSFS_SDK = $SdkRoot
    Push-Location $rfiRoot
    try {
        cargo build -p instrument-connector -p desktop-cli --features instrument-connector/msfs --release --locked
        if ($LASTEXITCODE -ne 0) { throw 'Native connector build failed' }
        $rfiStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
        $rfiPackage = Join-Path $rfiRoot "artifacts\native-connector-$rfiStamp"
        New-Item -ItemType Directory -Path $rfiPackage | Out-Null
        foreach ($rfiExe in @('instrument-connector.exe','desktop-cli.exe')) {
            Copy-Item -LiteralPath (Join-Path $rfiRoot "target\release\$rfiExe") -Destination $rfiPackage
        }
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'docs\NATIVE-CONNECTOR.md') -Destination (Join-Path $rfiPackage 'START-HERE.md')
        foreach ($rfiName in @('LICENSE','NOTICE','THIRD-PARTY-NOTICES.md')) {
            Copy-Item -LiteralPath (Join-Path $rfiRoot $rfiName) -Destination $rfiPackage
        }
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'fixtures\altimeter-lifecycle.txt') -Destination $rfiPackage
        if ($IncludeSdkRuntime) {
            Copy-Item -LiteralPath (Join-Path $SdkRoot 'SimConnect SDK\lib\SimConnect.dll') -Destination $rfiPackage
            'Unmodified Microsoft MSFS 2020 SDK runtime for local testing; Microsoft terms apply. The project license does not relicense this DLL.' | Set-Content -LiteralPath (Join-Path $rfiPackage 'MICROSOFT-RUNTIME.txt') -Encoding ASCII
        }
        python (Join-Path $PSScriptRoot 'snapshot_sources.py') $rfiPackage instrument-connector/msfs
        if ($LASTEXITCODE -ne 0) { throw 'Source snapshot failed' }
        python (Join-Path $PSScriptRoot 'package_native_dependencies.py') $rfiPackage
        if ($LASTEXITCODE -ne 0) { throw 'Dependency notices/source packaging failed' }
        $rfiFiles = Get-ChildItem -LiteralPath $rfiPackage -Recurse -File | ForEach-Object {
            [ordered]@{path=$_.FullName.Substring($rfiPackage.Length + 1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
        }
        [ordered]@{component='native-connector-auto-sync';branch=(git branch --show-current);base_commit=(git rev-parse HEAD);working_tree_modified=[bool](git status --porcelain);features=@('instrument-connector/msfs');includes_local_sdk_runtime=[bool]$IncludeSdkRuntime;firmware_included=$false;files=@($rfiFiles)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $rfiPackage 'manifest.json') -Encoding UTF8
        Compress-Archive -Path (Join-Path $rfiPackage '*') -DestinationPath "$rfiPackage.zip"
        Get-FileHash -LiteralPath "$rfiPackage.zip" -Algorithm SHA256 | Format-List
        Write-Output "Native test package: $rfiPackage.zip"
    } finally { Pop-Location }
} finally { $env:MSFS_SDK = $rfiPreviousSdk }
