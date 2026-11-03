param([string]$SdkRoot='C:\MSFS SDK')
$ErrorActionPreference='Stop'
$rfiRoot=Split-Path $PSScriptRoot -Parent
$rfiCompiler=Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
if (-not (Test-Path -LiteralPath $rfiCompiler)) { throw 'Install Inno Setup 6 first: winget install --id JRSoftware.InnoSetup --exact' }
$rfiPreviousSdk=$env:MSFS_SDK
Push-Location $rfiRoot
try {
    $env:MSFS_SDK=$SdkRoot
    cargo build -p instrument-tray -p instrument-connector -p desktop-cli --features instrument-connector/msfs --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Windows driver build failed' }
    $rfiOut=Join-Path $rfiRoot ('artifacts\windows-driver-'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
    $rfiStage=Join-Path $rfiOut 'package'
    New-Item -ItemType Directory -Path $rfiStage | Out-Null
    foreach ($rfiName in @('instrument-tray.exe','instrument-connector.exe')) {
        Copy-Item -LiteralPath (Join-Path $rfiRoot "target\release\$rfiName") -Destination $rfiStage
    }
    foreach ($rfiName in @('LICENSE','NOTICE','THIRD-PARTY-NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $rfiRoot $rfiName) -Destination $rfiStage
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Export-Diagnostics.ps1') -Destination $rfiStage
    Copy-Item -LiteralPath (Join-Path $rfiRoot 'docs\WINDOWS-DRIVER-SETUP.md') -Destination (Join-Path $rfiStage 'START-HERE.md')
    python (Join-Path $PSScriptRoot 'snapshot_sources.py') $rfiStage instrument-connector/msfs
    if ($LASTEXITCODE -ne 0) { throw 'Source snapshot failed' }
    python (Join-Path $PSScriptRoot 'package_native_dependencies.py') $rfiStage
    if ($LASTEXITCODE -ne 0) { throw 'Dependency packaging failed' }
    $rfiFiles=Get-ChildItem -LiteralPath $rfiStage -Recurse -File | ForEach-Object {
        [ordered]@{path=$_.FullName.Substring($rfiStage.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName).Hash}
    }
    [ordered]@{component='windows-instrument-driver';base_commit=(git rev-parse HEAD);branch=(git branch --show-current);working_tree_modified=[bool](git status --porcelain);firmware_required='T2 + active-only control claim';runtime_included=$false;files=@($rfiFiles)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $rfiStage 'manifest.json') -Encoding UTF8
    & $rfiCompiler "/DPackageDir=$rfiStage" "/DOutputDir=$rfiOut" (Join-Path $rfiRoot 'installer\round-instruments.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    Get-ChildItem -LiteralPath $rfiOut -Filter '*.exe' | Get-FileHash
    Write-Output "Driver installer: $rfiOut"
} finally { $env:MSFS_SDK=$rfiPreviousSdk; Pop-Location }
