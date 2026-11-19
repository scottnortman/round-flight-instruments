param([string]$SdkRoot='C:\MSFS SDK', [switch]$IncludeSdkRuntime, [string]$NativeTargetDir='', [string]$OutputDir='', [Parameter(Mandatory=$true)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$FirmwareElfSha256)
$ErrorActionPreference='Stop'
$rfiRoot=Split-Path $PSScriptRoot -Parent
$rfiCompiler=Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
if (-not (Test-Path -LiteralPath $rfiCompiler)) { throw 'Install Inno Setup 6 first: winget install --id JRSoftware.InnoSetup --exact' }
$rfiPreviousSdk=$env:MSFS_SDK
$rfiPreviousTarget=$env:CARGO_TARGET_DIR
$rfiPreviousFlags=$env:RUSTFLAGS
$rfiPreviousEncodedFlags=$env:CARGO_ENCODED_RUSTFLAGS
Push-Location $rfiRoot
try {
    $env:MSFS_SDK=$SdkRoot
    if (-not $NativeTargetDir) { $NativeTargetDir=if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $rfiRoot 'target' } }
    $rfiTarget=[IO.Path]::GetFullPath($NativeTargetDir)
    $env:CARGO_TARGET_DIR=$rfiTarget
    if ($env:CARGO_ENCODED_RUSTFLAGS) {
        $env:CARGO_ENCODED_RUSTFLAGS += [char]31 + '-C' + [char]31 + 'target-feature=+crt-static'
    } else { $env:RUSTFLAGS=($env:RUSTFLAGS+' -C target-feature=+crt-static').Trim() }
    $rfiMetadata = (& cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read package metadata' }
    $rfiVersion=($rfiMetadata.packages | Where-Object name -eq 'instrument-tray').version
    if ($rfiVersion -notmatch '^(\d+)\.(\d+)\.(\d+)-test\.(\d+)$') { throw 'Expected a versioned test release' }
    $rfiInstallerVersion=$Matches[1]+'.'+$Matches[2]+'.'+$Matches[3]+'.'+$Matches[4]
    $rfiOut=if ($OutputDir) { [IO.Path]::GetFullPath($OutputDir) } else { Join-Path $rfiRoot ('artifacts\windows-driver-'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff')) }
    if ((Test-Path -LiteralPath $rfiOut) -and @(Get-ChildItem -LiteralPath $rfiOut -Force).Count -gt 0) {
        throw 'Choose a new or empty output directory; existing packages are preserved.'
    }
    cargo build -p instrument-tray -p instrument-connector -p desktop-cli --bins --features instrument-connector/msfs --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Windows driver build failed' }
    $rfiStage=Join-Path $rfiOut 'package'
    New-Item -ItemType Directory -Path $rfiStage | Out-Null
    foreach ($rfiName in @('instrument-tray.exe','instrument-profile-editor.exe','instrument-connector.exe','desktop-cli.exe')) {
        Copy-Item -LiteralPath (Join-Path $rfiTarget "x86_64-pc-windows-msvc\release\$rfiName") -Destination $rfiStage
    }
    if ($IncludeSdkRuntime) {
        $rfiRuntime = Join-Path $SdkRoot 'SimConnect SDK\lib\SimConnect.dll'
        Copy-Item -LiteralPath $rfiRuntime -Destination $rfiStage
        Copy-Item -LiteralPath (Join-Path $SdkRoot 'Licenses\MSFS SDK EULA.pdf') -Destination (Join-Path $rfiStage 'MICROSOFT-SDK-LICENSE.pdf')
        'Unmodified Microsoft SimConnect runtime from the installed MSFS 2020 SDK. Microsoft terms apply; Apache-2.0 does not relicense it. Project maintainer confirmed permission to include this DLL on 2026-11-03. Preserve the accompanying Microsoft license and this notice.' | Set-Content -LiteralPath (Join-Path $rfiStage 'MICROSOFT-RUNTIME.txt') -Encoding ASCII
    }
    foreach ($rfiName in @('LICENSE','NOTICE','THIRD-PARTY-NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $rfiRoot $rfiName) -Destination $rfiStage
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Export-Diagnostics.ps1') -Destination $rfiStage
    foreach ($rfiGuide in @('START-HERE.md','START-HERE.txt','TEST-INSTRUCTIONS.md')) {
        Copy-Item -LiteralPath (Join-Path $rfiRoot 'docs\MSFS-TEST-13.md') -Destination (Join-Path $rfiStage $rfiGuide)
    }
    $rfiGuides=Join-Path $rfiStage 'guides'
    New-Item -ItemType Directory -Path $rfiGuides | Out-Null
    foreach ($rfiGuide in @('DEMO.md','CONSOLIDATED.md','DEBUG.md','PROFILE-EDITOR.md')) {
        Copy-Item -LiteralPath (Join-Path $rfiRoot ('docs\'+$rfiGuide)) -Destination $rfiGuides
    }
    [ordered]@{version=$rfiVersion;installer_version=$rfiInstallerVersion;target='x86_64-pc-windows-msvc';c_runtime='static';screens=8;profile_delivery='manual-selection-session-acknowledged';firmware_update_required=$true;qualified_firmware_elf_sha256=$FirmwareElfSha256.ToUpperInvariant();menu_hold_ms=1000} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $rfiStage 'release-info.json') -Encoding UTF8
    python (Join-Path $PSScriptRoot 'snapshot_sources.py') $rfiStage instrument-connector/msfs
    if ($LASTEXITCODE -ne 0) { throw 'Source snapshot failed' }
    python (Join-Path $PSScriptRoot 'package_native_dependencies.py') $rfiStage
    if ($LASTEXITCODE -ne 0) { throw 'Dependency packaging failed' }
    $rfiFiles=Get-ChildItem -LiteralPath $rfiStage -Recurse -File | ForEach-Object {
        [ordered]@{path=$_.FullName.Substring($rfiStage.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName).Hash}
    }
    [ordered]@{component='windows-instrument-driver';version=$rfiVersion;target='x86_64-pc-windows-msvc';c_runtime='static';base_commit=(git rev-parse HEAD);branch=(git branch --show-current);working_tree_modified=[bool](git status --porcelain);firmware_required='T2 + active-only control claim';runtime_included=[bool]$IncludeSdkRuntime;runtime_source_sdk=(Get-Content (Join-Path $SdkRoot 'version.txt') -Raw).Trim();files=@($rfiFiles)} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $rfiStage 'manifest.json') -Encoding UTF8
    & $rfiCompiler /Q "/DPackageDir=$rfiStage" "/DOutputDir=$rfiOut" "/DReleaseVersion=$rfiVersion" "/DInstallerVersion=$rfiInstallerVersion" (Join-Path $rfiRoot 'installer\round-instruments.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
    Get-ChildItem -LiteralPath $rfiOut -Filter '*.exe' | Get-FileHash
    Write-Output "Driver installer: $rfiOut"
} finally { $env:MSFS_SDK=$rfiPreviousSdk; $env:CARGO_TARGET_DIR=$rfiPreviousTarget; $env:RUSTFLAGS=$rfiPreviousFlags; $env:CARGO_ENCODED_RUSTFLAGS=$rfiPreviousEncodedFlags; Pop-Location }
