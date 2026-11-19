param([string]$OutputDir='', [string]$NativeTargetDir='')
$ErrorActionPreference='Stop'
$rfiProfileRoot=Split-Path $PSScriptRoot -Parent
$rfiProfilePreviousTarget=$env:CARGO_TARGET_DIR
$rfiProfilePreviousFlags=$env:RUSTFLAGS
$rfiProfilePreviousEncodedFlags=$env:CARGO_ENCODED_RUSTFLAGS
Push-Location $rfiProfileRoot
try {
    if ($NativeTargetDir) { $env:CARGO_TARGET_DIR=[IO.Path]::GetFullPath($NativeTargetDir) }
    if ($env:CARGO_ENCODED_RUSTFLAGS) {
        $env:CARGO_ENCODED_RUSTFLAGS += [char]31 + '-C' + [char]31 + 'target-feature=+crt-static'
    } else { $env:RUSTFLAGS=($env:RUSTFLAGS+' -C target-feature=+crt-static').Trim() }
    $rfiProfileTarget=if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $rfiProfileRoot 'target' }
    cargo build -p instrument-tray --bin instrument-profile-editor --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Profile editor build failed' }
    $rfiProfileOut=if ($OutputDir) { [IO.Path]::GetFullPath($OutputDir) } else { Join-Path $rfiProfileRoot ('artifacts\profile-editor-'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff')) }
    if ((Test-Path -LiteralPath $rfiProfileOut) -and @(Get-ChildItem -LiteralPath $rfiProfileOut -Force).Count -gt 0) {
        throw 'Choose a new or empty package directory; existing files are preserved.'
    }
    New-Item -ItemType Directory -Path $rfiProfileOut -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $rfiProfileTarget 'x86_64-pc-windows-msvc\release\instrument-profile-editor.exe') -Destination $rfiProfileOut
    foreach ($rfiNotice in @('LICENSE','NOTICE','THIRD-PARTY-NOTICES.md')) {
        Copy-Item -LiteralPath (Join-Path $rfiProfileRoot $rfiNotice) -Destination $rfiProfileOut
    }
    Copy-Item -LiteralPath (Join-Path $rfiProfileRoot 'docs\PROFILE-EDITOR.md') -Destination (Join-Path $rfiProfileOut 'START-HERE.md')
    python (Join-Path $PSScriptRoot 'snapshot_sources.py') $rfiProfileOut none
    if ($LASTEXITCODE -ne 0) { throw 'Profile editor source snapshot failed' }
    python (Join-Path $PSScriptRoot 'package_native_dependencies.py') $rfiProfileOut instrument-tray
    if ($LASTEXITCODE -ne 0) { throw 'Profile editor dependency notices failed' }
    $rfiProfileFiles=Get-ChildItem -LiteralPath $rfiProfileOut -Recurse -File | ForEach-Object {
        [ordered]@{path=$_.FullName.Substring($rfiProfileOut.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName).Hash}
    }
    [ordered]@{component='aircraft-profile-editor-preview';base_commit=(git rev-parse HEAD);branch=(git branch --show-current);working_tree_modified=[bool](git status --porcelain);target='x86_64-pc-windows-msvc';c_runtime='static';schema_version=1;live_delivery=$false;simconnect_required=$false;firmware_included=$false;files=@($rfiProfileFiles)} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $rfiProfileOut 'manifest.json') -Encoding UTF8
    Compress-Archive -Path (Join-Path $rfiProfileOut '*') -DestinationPath ($rfiProfileOut+'.zip')
    Get-FileHash -LiteralPath ($rfiProfileOut+'.zip') | Select-Object Hash,Path
    Write-Output ('Standalone editor: '+(Join-Path $rfiProfileOut 'instrument-profile-editor.exe'))
} finally { $env:CARGO_TARGET_DIR=$rfiProfilePreviousTarget; $env:RUSTFLAGS=$rfiProfilePreviousFlags; $env:CARGO_ENCODED_RUSTFLAGS=$rfiProfilePreviousEncodedFlags; Pop-Location }
