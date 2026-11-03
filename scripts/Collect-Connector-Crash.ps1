param([string]$OutputPath = (Join-Path $PSScriptRoot 'connector-crash-events.json'))
$ErrorActionPreference = 'Stop'
$rfiNow = Get-Date
$rfiSince = $rfiNow.AddDays(-2)
$rfiEvents = @()
$rfiQueryError = $null
try {
    $rfiEvents = @(Get-WinEvent -FilterHashtable @{
        LogName = 'Application'; Id = @(1000,1001,1002); StartTime = $rfiSince
    } -ErrorAction Stop | Where-Object {
        # Restrict collection to this executable, including localized event data.
        $_.ToXml() -match '(?i)instrument-connector\.exe'
    } | Select-Object -First 10 | ForEach-Object {
        [ordered]@{
            time_created = $_.TimeCreated.ToString('o')
            id = $_.Id
            provider = $_.ProviderName
            message = $_.Message
            xml = $_.ToXml()
        }
    })
} catch {
    $rfiQueryError = $_.Exception.Message
}
[ordered]@{
    collected_at = $rfiNow.ToString('o')
    since = $rfiSince.ToString('o')
    query_error = $rfiQueryError
    events = @($rfiEvents)
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
Write-Host "Saved $($rfiEvents.Count) connector event(s) to $OutputPath"
if ($rfiQueryError) { Write-Host "Event query: $rfiQueryError" }
Write-Host 'Return this JSON file. No simulator run or device connection is needed.'
