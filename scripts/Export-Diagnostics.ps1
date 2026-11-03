param([Parameter(Mandatory=$true)][string]$DataDirectory,
      [Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
# Export only app-owned configuration and text logs; never DLLs/executables.
$rfiData = (Resolve-Path -LiteralPath $DataDirectory).Path
$rfiStage = Join-Path ([IO.Path]::GetTempPath()) ('rfi-diagnostics-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $rfiStage | Out-Null
try {
    foreach ($rfiName in @('settings.json','device.json','last-message.txt')) {
        $rfiFile = Join-Path $rfiData $rfiName
        if (Test-Path -LiteralPath $rfiFile -PathType Leaf) { Copy-Item -LiteralPath $rfiFile -Destination $rfiStage }
    }
    $rfiLogs = Join-Path $rfiData 'logs'
    if (Test-Path -LiteralPath $rfiLogs) {
        # Most recent five sessions, bounded to 128 MiB, newest files first.
        [long]$rfiBytes = 0
        foreach ($rfiSession in (Get-ChildItem -LiteralPath $rfiLogs -Directory | Sort-Object LastWriteTime -Descending | Select-Object -First 5)) {
            if ($rfiSession.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            $rfiTarget = Join-Path $rfiStage $rfiSession.Name
            New-Item -ItemType Directory -Path $rfiTarget | Out-Null
            foreach ($rfiFile in (Get-ChildItem -LiteralPath $rfiSession.FullName -File | Sort-Object LastWriteTime -Descending)) {
                if ($rfiFile.Extension -notin @('.json','.jsonl','.txt','.log')) { continue }
                if ($rfiFile.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
                if (($rfiBytes + $rfiFile.Length) -gt 128MB) { continue }
                Copy-Item -LiteralPath $rfiFile.FullName -Destination $rfiTarget
                $rfiBytes += $rfiFile.Length
            }
        }
    }
    'Latest five sessions, up to 128 MiB. USB serial and local paths are included. No executables or simulator runtime.' | Set-Content -LiteralPath (Join-Path $rfiStage 'CONTENTS.txt')
    Compress-Archive -Path (Join-Path $rfiStage '*') -DestinationPath $Destination
} finally {
    $rfiResolved = [IO.Path]::GetFullPath($rfiStage)
    $rfiTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $rfiResolved.StartsWith($rfiTemp,[StringComparison]::OrdinalIgnoreCase) -or (Split-Path $rfiResolved -Leaf) -notlike 'rfi-diagnostics-*') { throw 'Refusing unexpected cleanup path' }
    Remove-Item -LiteralPath $rfiResolved -Recurse -Force
}
