param([switch]$WatchdogSelfTest)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Enter-DevShell.ps1')
$rfiPreviousTarget = $env:CARGO_TARGET_DIR
$rfiPreviousDefaults = $env:ESP_IDF_SDKCONFIG_DEFAULTS
$rfiPreviousRevision = $env:RFI_BUILD_REV
try {
    $rfiSourceRoot = Split-Path $PSScriptRoot -Parent
    $env:RFI_BUILD_REV = 'source-archive'
    if ((Test-Path -LiteralPath (Join-Path $rfiSourceRoot '.git')) -and (Get-Command git -ErrorAction SilentlyContinue)) {
        $env:RFI_BUILD_REV = (& git -C $rfiSourceRoot describe --tags --long --always --dirty --abbrev=8).Trim()
        if ($LASTEXITCODE -ne 0) { throw 'Could not identify firmware source revision' }
    }
    $env:CARGO_TARGET_DIR = 'D:\rfi-c'
    New-Item -ItemType Directory -Path $env:CARGO_TARGET_DIR -Force | Out-Null
    # esp-idf-sys creates its CMake project outside this checkout. Resolve the
    # canonical CSV here rather than depending on that generated project path.
    $rfiConsole = Join-Path (Split-Path $PSScriptRoot -Parent) 'firmware\console'
    $rfiCsv = (Resolve-Path (Join-Path $rfiConsole 'partitions.csv')).Path
    $rfiDefaults = (Get-Content (Join-Path $rfiConsole 'sdkconfig.defaults') -Raw).Replace('CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="partitions.csv"', ('CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="' + $rfiCsv.Replace('\','/') + '"'))
    $env:ESP_IDF_SDKCONFIG_DEFAULTS = Join-Path $env:CARGO_TARGET_DIR 'rfi-sdkconfig.defaults'
    if (-not (Test-Path $env:ESP_IDF_SDKCONFIG_DEFAULTS) -or (Get-Content $env:ESP_IDF_SDKCONFIG_DEFAULTS -Raw) -cne $rfiDefaults) {
        [IO.File]::WriteAllText($env:ESP_IDF_SDKCONFIG_DEFAULTS, $rfiDefaults)
    }
    Push-Location (Join-Path (Split-Path $PSScriptRoot -Parent) 'firmware\console')
    try {
        $rfiBuildArgs = @('build','--locked','--message-format=json')
        if ($WatchdogSelfTest) { $rfiBuildArgs += @('--features','watchdog-self-test') }
        $rfiIdfOut = $null
        $rfiExecutable = $null
        # Cargo reports the exact build-script output even for cached builds.
        # Do not select the newest SDK directory or mix artifacts across flags.
        cargo @rfiBuildArgs | ForEach-Object {
            $rfiMessage = $_ | ConvertFrom-Json
            if ($rfiMessage.reason -eq 'build-script-executed' -and $rfiMessage.package_id -match 'esp-idf-sys[@:]') { $rfiIdfOut = $rfiMessage.out_dir }
            if ($rfiMessage.reason -eq 'compiler-artifact' -and $rfiMessage.target.name -eq 'rfi-console') { $rfiExecutable = $rfiMessage.executable }
            if ($rfiMessage.reason -eq 'compiler-message') { Write-Host $rfiMessage.message.rendered }
        }
        if ($LASTEXITCODE -ne 0) { throw "Console build failed: $LASTEXITCODE" }
        # Check generated settings, not only requested defaults: shared Cargo
        # caches can otherwise retain another application's SDK configuration.
        if (-not $rfiIdfOut -or -not $rfiExecutable) { throw 'Cargo did not identify matching IDF output and executable' }
        $rfiSdk = Join-Path $rfiIdfOut 'sdkconfig'
        $rfiBoot = Join-Path $rfiIdfOut 'build\bootloader\bootloader.bin'
        $rfiPartition = Join-Path $rfiIdfOut 'build\partition_table\partition-table.bin'
        python (Join-Path $PSScriptRoot 'partition_layout.py') --csv $rfiCsv --binary $rfiPartition --sdkconfig $rfiSdk
        if ($LASTEXITCODE -ne 0) { throw 'Generated partition table verification failed' }
        if (-not (Test-Path -LiteralPath $rfiBoot)) { throw 'Matching bootloader missing' }
        $rfiConfig = Get-Content -LiteralPath $rfiSdk
        foreach ($rfiRequired in @(
            'CONFIG_ESP_MAIN_TASK_STACK_SIZE=16384',
            'CONFIG_ESP_TASK_WDT_EN=y',
            'CONFIG_ESP_TASK_WDT_INIT=y',
            'CONFIG_ESP_TASK_WDT_PANIC=y',
            'CONFIG_ESP_TASK_WDT_TIMEOUT_S=1',
            'CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0=y',
            'CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU1=y',
            'CONFIG_FREERTOS_HZ=1000',
            'CONFIG_ESP_CONSOLE_UART_BAUDRATE=115200',
            'CONFIG_ESPTOOLPY_FLASHSIZE_16MB=y',
            'CONFIG_SPIRAM=y',
            'CONFIG_SPIRAM_MODE_OCT=y',
            'CONFIG_SPIRAM_SPEED_80M=y',
            'CONFIG_SPIRAM_USE_MALLOC=y',
            'CONFIG_SPIRAM_XIP_FROM_PSRAM=y',
            'CONFIG_SPIRAM_FETCH_INSTRUCTIONS=y',
            'CONFIG_SPIRAM_RODATA=y',
            'CONFIG_ESP32S3_DATA_CACHE_LINE_64B=y',
            'CONFIG_ESP32S3_DATA_CACHE_32KB=y',
            'CONFIG_ESP32S3_INSTRUCTION_CACHE_32KB=y',
            'CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ=240'
            '# CONFIG_LCD_RGB_RESTART_IN_VSYNC is not set'
            'CONFIG_I2C_SKIP_LEGACY_CONFLICT_CHECK=y'
        )) {
            if ($rfiConfig -notcontains $rfiRequired) { throw "Generated SDK config missing: $rfiRequired" }
        }
        # The IDF legacy constructor is retained by whole-archive linkage even
        # when no legacy I2C function is used. Replace that startup false positive
        # with a build-time prohibition on the actual legacy implementation.
        $rfiNm = 'D:\rfi-e\tools\xtensa-esp-elf\esp-14.2.0_20260121\xtensa-esp-elf\bin\xtensa-esp32s3-elf-nm.exe'
        $rfiSymbols = & $rfiNm --defined-only $rfiExecutable
        if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect firmware I2C symbols' }
        if ($rfiSymbols -match '\b(i2c_driver_install|i2c_param_config|i2c_master_cmd_begin|i2c_isr_handler_default)$') {
            throw 'Legacy I2C implementation linked alongside master driver'
        }
        if (-not ($rfiSymbols -match '\bi2c_new_master_bus$')) { throw 'Current I2C master driver missing' }
        [ordered]@{executable=$rfiExecutable;sdkconfig=$rfiSdk;bootloader=$rfiBoot;bootloader_sha256=(Get-FileHash $rfiBoot).Hash;partition_csv=$rfiCsv;partition_csv_sha256=(Get-FileHash $rfiCsv).Hash;partition_binary=$rfiPartition;partition_binary_sha256=(Get-FileHash $rfiPartition).Hash;watchdog_self_test=[bool]$WatchdogSelfTest;sha256=(Get-FileHash -LiteralPath $rfiExecutable).Hash} | ConvertTo-Json | Set-Content -LiteralPath 'D:\rfi-c\console-build-artifacts.json' -Encoding UTF8
        Write-Output 'Console build and generated SDK safety settings verified.'
    } finally { Pop-Location }
} finally { $env:CARGO_TARGET_DIR = $rfiPreviousTarget; $env:ESP_IDF_SDKCONFIG_DEFAULTS = $rfiPreviousDefaults; $env:RFI_BUILD_REV = $rfiPreviousRevision }
