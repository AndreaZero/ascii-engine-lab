param(
    [int]$Frames = 120,
    [int]$Repetitions = 3,
    [int[]]$Workers = @(1, 2, 4, 8, 16),
    [switch]$BaselineOnly,
    [string]$Output = 'evidence/r2/scaling'
)
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $labRoot
$outputRoot = [IO.Path]::GetFullPath((Join-Path $labRoot $Output))
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$variants = if ($BaselineOnly) { @(0) } else { $Workers }
$logical = [Environment]::ProcessorCount
for ($round = 1; $round -le $Repetitions; $round++) {
    # Alternate the order to reduce the confounding of warm-up/thermal drift.
    $order = @($variants)
    if ($round % 2 -eq 0) { [array]::Reverse($order) }
    foreach ($workersCount in $order) {
        foreach ($config in @(@(1280,720,160,90), @(1920,1080,240,135))) {
            $label = if ($BaselineOnly) { 'r1' } else { "w$workersCount" }
            $destination = Join-Path $outputRoot "$label-$($config[1])p-r$round"
            New-Item -ItemType Directory -Force -Path $destination | Out-Null
            $executable = if ($BaselineOnly) {
                Join-Path $labRoot 'evidence/r2/baseline/ascii-engine-lab-r1.exe'
            } else { Join-Path $labRoot 'target/release/ascii-engine-lab.exe' }
            $arguments = @('--benchmark', '--frames', $Frames, '--width', $config[0], '--height', $config[1], '--cols', $config[2], '--rows', $config[3], '--output', ('"' + $destination + '"'))
            if (-not $BaselineOnly) { $arguments += @('--workers', $workersCount) }
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $process = Start-Process -FilePath $executable -ArgumentList $arguments -PassThru -WindowStyle Hidden -RedirectStandardOutput (Join-Path $destination 'stdout.txt') -RedirectStandardError (Join-Path $destination 'stderr.txt')
            $process.WaitForExit()
            $timer.Stop()
            if ($process.ExitCode -ne 0) { throw "Benchmark failed: $destination, exit $($process.ExitCode)" }
            $cpuSeconds = $null
            $cpuError = $null
            try { $cpuSeconds = $process.TotalProcessorTime.TotalSeconds } catch { $cpuError = $_.Exception.Message }
            $processReport = [ordered]@{
                executable = $executable
                executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
                arguments = $arguments
                round = $round
                logical_cpus = $logical
                wall_seconds = $timer.Elapsed.TotalSeconds
                process_cpu_seconds = $cpuSeconds
                average_busy_logical_cpu_equivalents = if ($null -ne $cpuSeconds) { $cpuSeconds / $timer.Elapsed.TotalSeconds } else { $null }
                process_cpu_percent_of_logical_capacity = if ($null -ne $cpuSeconds) { 100 * $cpuSeconds / $timer.Elapsed.TotalSeconds / $logical } else { $null }
                cpu_counter_error = $cpuError
                scope = 'Whole process: 12 cases, startup, captures, warmups, CPU generation, GPU driver/waits and file writes. Not isolated composer utilization or physical-core occupancy.'
            }
            $processReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $destination 'process.json') -Encoding utf8
            Write-Output "$label $($config[1])p round $round done ($([Math]::Round($timer.Elapsed.TotalSeconds,1)) s)"
            $process.Dispose()
        }
    }
}
