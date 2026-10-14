# Windows PowerShell 5.1+; no compiler, package manager or network access needed.
[CmdletBinding()]
param(
    [string]$PackageDirectory,
    [string]$OutputDirectory,
    [ValidateSet('dx12','vulkan','auto','metal','gl')][string]$Backend = 'dx12',
    [ValidateSet('economy','balanced','detail')][string]$Profile = 'balanced',
    [ValidatePattern('^(auto|[1-9][0-9]?)$')][string]$Workers = 'auto',
    [switch]$Windows,
    [ValidateRange(1,30)][int]$WindowSeconds = 3,
    [ValidateRange(10,1800)][int]$TimeoutSeconds = 120,
    [ValidateSet('unspecified','development-host','other-physical-host')][string]$MachineRole = 'unspecified',
    [string]$MachineLabel = '',
    [string]$LoadNotes = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Windows PowerShell 5.1 evaluates parameter defaults before PSScriptRoot is
# populated. Resolve this default in the script body so direct -File launches
# work just like Probe.cmd, which supplies PackageDirectory explicitly.
if ([string]::IsNullOrWhiteSpace($PackageDirectory)) { $PackageDirectory = $PSScriptRoot }
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path.TrimEnd('\', '/')
if (!$OutputDirectory) { $OutputDirectory = Join-Path $package ('results/probe-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)) }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw "Probe output must be a fresh directory: $output" }
$null = New-Item -ItemType Directory -Path $output
$utf8 = New-Object Text.UTF8Encoding($false)
$script:records = @()
$report = [ordered]@{
    schema_version=1; started_utc=[DateTime]::UtcNow.ToString('o'); finished_utc=$null; status='RUNNING'
    machine_role_self_reported=$MachineRole; machine_label=$MachineLabel; load_notes=$LoadNotes
    host=[ordered]@{ os=$null; cpu=@(); gpu_inventory=@(); powershell=$PSVersionTable.PSVersion.ToString(); is_64bit_process=[Environment]::Is64BitProcess; is_64bit_os=[Environment]::Is64BitOperatingSystem; collection_errors=@() }
    configuration=[ordered]@{ backend_requested=$Backend; profile=$Profile; workers_requested=$Workers; sampling='center'; windows_requested=[bool]$Windows; window_seconds=$WindowSeconds; timeout_seconds=$TimeoutSeconds; wgpu_backend_environment=$env:WGPU_BACKEND }
    package_manifest_sha256=$null; package_source=$null; payload_files=0; runtime_prerequisites=$null
    cases=@(); errors=@()
    limits=@('Machine role is an operator declaration, not independently verified hardware identity.', 'A directory move, VM or worker override does not validate a second physical host.', 'Headless images do not establish visible window quality or frame rate.', 'Window exits/logs are smoke evidence; inspect captures and interactive behavior separately.', 'A selected backend failure is recorded; no backend/quality fallback is attempted.', 'Logs and native reports may contain local paths. Review before publishing.')
}
function Write-Utf8([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, $utf8) }
function Save-Report {
    $report.cases = @($script:records)
    Write-Utf8 (Join-Path $output 'probe.json') (($report | ConvertTo-Json -Depth 30) + "`n")
}
function Assert-PayloadPath([string]$Relative) {
    if (!$Relative -or $Relative.Contains('\') -or $Relative.Contains(':') -or $Relative.StartsWith('/') -or @($Relative.Split('/') | Where-Object { $_ -eq '' -or $_ -eq '.' -or $_ -eq '..' }).Count -gt 0) { throw "Invalid manifest path: $Relative" }
    $resolved = [IO.Path]::GetFullPath((Join-Path $package $Relative))
    if (!$resolved.StartsWith($package + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Manifest path escapes package: $Relative" }
    $cursor = $resolved
    while ($cursor.Length -ge $package.Length) {
        if ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in payload: $Relative" }
        if ($cursor -eq $package) { break }
        $cursor = Split-Path -Parent $cursor
    }
    return $resolved
}
function Quote-NativeArgument([string]$Value) {
    # Win32 CommandLineToArgvW-compatible quoting, including trailing backslashes.
    $builder = New-Object Text.StringBuilder
    $null = $builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') {
            $null = $builder.Append([char]92, 2 * $slashes + 1).Append('"')
        } else {
            $null = $builder.Append([char]92, $slashes).Append($character)
        }
        $slashes = 0
    }
    $null = $builder.Append([char]92, 2 * $slashes).Append('"')
    return $builder.ToString()
}
function Invoke-ProbeCase([string]$Name, [string]$Executable, [string[]]$Arguments, [string]$Kind, [string[]]$ExpectedFiles) {
    $folder = Join-Path $output ('cases/' + $Name)
    $cwd = Join-Path $folder 'cwd'
    $null = New-Item -ItemType Directory -Path $cwd
    $argumentsWithOutput = @($Arguments)
    if ($Kind -ne 'author') { $argumentsWithOutput += @('--output', $folder) }
    $record = [ordered]@{ name=$Name; executable=$Executable; kind=$Kind; arguments=@($argumentsWithOutput | ForEach-Object { $_.Replace($package, '<package>').Replace($output, '<results>') }); status='RUNNING'; started_utc=[DateTime]::UtcNow.ToString('o'); elapsed_s=$null; exit_code=$null; failure_stage=$null; error=$null; stdout="cases/$Name/stdout.txt"; stderr="cases/$Name/stderr.txt"; artifacts=@(); native_reports=@() }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = New-Object Diagnostics.Process
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = Join-Path $package $Executable
    $start.Arguments = (@($argumentsWithOutput | ForEach-Object { Quote-NativeArgument $_ }) -join ' ')
    $start.WorkingDirectory = $cwd
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = $utf8
    $start.StandardErrorEncoding = $utf8
    $process.StartInfo = $start
    try {
        if (!$process.Start()) { throw 'Process.Start returned false' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            $record.failure_stage = 'timeout'
            $process.Kill()
            $process.WaitForExit()
        }
        $record.exit_code = $process.ExitCode
        Write-Utf8 (Join-Path $folder 'stdout.txt') $stdout.GetAwaiter().GetResult()
        Write-Utf8 (Join-Path $folder 'stderr.txt') $stderr.GetAwaiter().GetResult()
        if ($record.failure_stage -eq 'timeout') { throw "Exceeded $TimeoutSeconds seconds; only the process started for this case was terminated" }
        if ($process.ExitCode -ne 0) {
            $record.failure_stage = 'process-' + $Kind
            if ($process.ExitCode -in @(-1073741515, -1073741701, -1073741502)) {
                $record.failure_stage = 'windows-loader'
                throw 'Windows loader failure: check x64 architecture, Microsoft Visual C++ x64 runtime/UCRT and DLL integrity; see exit code and logs. No runtime was installed by this probe.'
            }
            throw "Native process failed with exit $($process.ExitCode); see stdout/stderr (GPU initialization may fail before native JSON exists)"
        }
        foreach ($relative in $ExpectedFiles) {
            $path = Join-Path $folder $relative
            if (!(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) {
                $record.failure_stage = 'evidence'
                throw "Successful exit did not produce required artifact $relative"
            }
        }
        foreach ($item in Get-ChildItem -LiteralPath $folder -Recurse -File) {
            $relative = $item.FullName.Substring($output.Length + 1).Replace('\', '/')
            $record.artifacts += [ordered]@{ path=$relative; bytes=$item.Length; sha256=(Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
            if ($item.Name -in @('runtime.json','run.json','window-run.json')) {
                $native = Get-Content -LiteralPath $item.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
                $record.native_reports += [ordered]@{ path=$relative; content=$native }
            }
        }
        $record.status = 'PASS'
    } catch {
        if (!$record.failure_stage) { $record.failure_stage = 'launch-or-collection' }
        $record.error = $_.Exception.Message
        $record.status = 'FAIL'
    } finally {
        $timer.Stop()
        $record.elapsed_s = $timer.Elapsed.TotalSeconds
        $process.Dispose()
        $script:records += $record
        Save-Report
        Write-Output ($Name + ': ' + $record.status + $(if ($record.error) { ' - ' + $record.error } else { '' }))
    }
}
$phase = 'host-inventory'
try {
    foreach ($source in @('Win32_OperatingSystem','Win32_Processor','Win32_VideoController')) {
        try {
            switch ($source) {
                'Win32_OperatingSystem' { $report.host.os = Get-CimInstance $source | Select-Object Caption, Version, BuildNumber, OSArchitecture, TotalVisibleMemorySize }
                'Win32_Processor' { $report.host.cpu = @(Get-CimInstance $source | Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors) }
                'Win32_VideoController' { $report.host.gpu_inventory = @(Get-CimInstance $source | Select-Object Name, DriverVersion, AdapterCompatibility) }
            }
        } catch { $report.host.collection_errors += "$source`: $($_.Exception.Message)" }
    }
    $phase = 'package-integrity'
    $manifestPath = Assert-PayloadPath 'package-manifest.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.schema_version -ne 1 -or $manifest.target -ne 'x86_64-pc-windows-msvc') { throw 'Unsupported package manifest/target' }
    $report.package_manifest_sha256 = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $report.package_source = $manifest.source
    $seen = @{}
    foreach ($entry in $manifest.files) {
        if ($seen.ContainsKey($entry.path)) { throw "Case-insensitive duplicate manifest entry: $($entry.path)" }
        $seen[$entry.path] = $true
        $path = Assert-PayloadPath $entry.path
        if ((Get-Item -LiteralPath $path).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256) { throw "Package integrity mismatch: $($entry.path)" }
    }
    foreach ($required in @('ascii-material-room.exe','ascii-engine-lab.exe','author.exe','LICENSE','PORTABLE_PACKAGE.md','probe-material-room.ps1','projects/starter.project.json','third-party-licenses/manifest.json')) {
        if (!$seen.ContainsKey($required)) { throw "Required file absent from manifest: $required" }
    }
    foreach ($item in Get-ChildItem -LiteralPath $package -Recurse -File -Force) {
        $relative = $item.FullName.Substring($package.Length + 1).Replace('\','/')
        if ($relative -eq 'package-manifest.json' -or $relative.StartsWith('results/') -or $relative.StartsWith('work/')) { continue }
        if (!$seen.ContainsKey($relative)) { throw "Unlisted package file: $relative (use work/ for your own projects)" }
    }
    $report.payload_files = $seen.Count
    $phase = 'runtime-prerequisites'
    if (![Environment]::Is64BitOperatingSystem -or ![Environment]::Is64BitProcess -or $env:OS -ne 'Windows_NT') { throw 'This package/probe requires Windows x64 and a 64-bit PowerShell process' }
    $runtime = Join-Path ([Environment]::SystemDirectory) 'VCRUNTIME140.dll'
    $report.runtime_prerequisites = [ordered]@{ vcruntime140_in_system32=(Test-Path -LiteralPath $runtime -PathType Leaf); universal_crt='Required; availability is exercised by native process launch'; note='File presence alone is not proof that DLL loading succeeds' }
    if (!$report.runtime_prerequisites.vcruntime140_in_system32 -and !(Test-Path -LiteralPath (Join-Path $package 'VCRUNTIME140.dll'))) { throw 'Missing VCRUNTIME140.dll: Microsoft Visual C++ x64 runtime is required. No application cases were started; install/repair the official runtime separately and repeat in a fresh output directory.' }
    $phase = 'case-orchestration'
    $project = Assert-PayloadPath $manifest.starter_project
    Invoke-ProbeCase 'author-inspect' 'author.exe' @('inspect',$project) 'author' @()
    Invoke-ProbeCase 'room-validate-project' 'ascii-material-room.exe' @('--project',$project,'--validate','--workers',$Workers) 'validation' @()
    Invoke-ProbeCase 'room-validate-embedded' 'ascii-material-room.exe' @('--validate','--workers',$Workers) 'validation' @()
    foreach ($view in @('first','third','overhead')) {
        $common = @('--camera',$view,'--profile',$Profile,'--workers',$Workers,'--backend',$Backend)
        Invoke-ProbeCase ('room-' + $view) 'ascii-material-room.exe' (@('--project',$project,'--headless','--frames','3','--sampling','center') + $common) 'headless' @('run.json',"$view/frame-0000.png", "$view/frame-0002.png")
        Invoke-ProbeCase ('lab-' + $view) 'ascii-engine-lab.exe' (@('--headless','--stage','cloth') + $common) 'headless' @('runtime.json',"$view-cloth.png")
    }
    if ($Windows) {
        foreach ($view in @('first','third','overhead')) {
            $common = @('--camera',$view,'--profile',$Profile,'--workers',$Workers,'--backend',$Backend,'--seconds',[string]$WindowSeconds)
            Invoke-ProbeCase ('room-window-' + $view) 'ascii-material-room.exe' (@('--project',$project,'--sampling','center') + $common) 'window' @('window-run.json')
            # The lab currently has no structured window report: stdout/exit only, explicitly labelled smoke.
            Invoke-ProbeCase ('lab-window-' + $view) 'ascii-engine-lab.exe' (@('--stage','cloth') + $common) 'window' @()
        }
    }
    $report.status = $(if (@($script:records | Where-Object { $_.status -ne 'PASS' }).Count -eq 0) { 'PASS' } else { 'FAIL' })
} catch {
    $report.status = 'FAIL'
    $report.errors += [ordered]@{ stage=$phase; message=$_.Exception.Message }
    Write-Warning $_.Exception.Message
} finally {
    $report.finished_utc = [DateTime]::UtcNow.ToString('o')
    Save-Report
}
Write-Output ('Probe ' + $report.status + ': ' + (Join-Path $output 'probe.json'))
if ($report.status -ne 'PASS') { exit 1 }
