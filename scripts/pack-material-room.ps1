# Assemble already-built Windows x64 binaries. Never builds, installs or downloads.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [Alias('ReleaseDir')][string]$ReleaseDirectory,
    [Alias('LicenseDir')][string]$LicenseDirectory,
    [switch]$NoArchive
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = Split-Path -Parent $PSScriptRoot
if (!$ReleaseDirectory) { $ReleaseDirectory = Join-Path $root 'target/release' }
if (!$LicenseDirectory) { $LicenseDirectory = Join-Path $root 'evidence/r15/package-licenses' }
$release = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$licenses = (Resolve-Path -LiteralPath $LicenseDirectory).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
$archive = $output.TrimEnd('\', '/') + '.zip'
if (Test-Path -LiteralPath $output) { throw "Destination already exists; choose a fresh directory: $output" }
if (!$NoArchive -and (Test-Path -LiteralPath $archive)) { throw "Archive already exists: $archive" }
if (!$NoArchive -and (Test-Path -LiteralPath ($archive + '.sha256'))) { throw 'Archive hash sidecar already exists' }
$utf8 = New-Object Text.UTF8Encoding($false)
function Write-Utf8([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, $utf8) }
function Assert-Regular([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point is not package input: $Path" }
}
function Assert-Amd64([string]$Path) {
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 64 -or $bytes[0] -ne 77 -or $bytes[1] -ne 90) { throw "Not a PE executable: $Path" }
    $pe = [BitConverter]::ToInt32($bytes, 60)
    if ($pe -lt 0 -or $pe -gt $bytes.Length - 26 -or [BitConverter]::ToUInt32($bytes, $pe) -ne 17744 -or [BitConverter]::ToUInt16($bytes, $pe + 4) -ne 34404 -or [BitConverter]::ToUInt16($bytes, $pe + 24) -ne 523) {
        throw "Expected AMD64 PE32+ executable: $Path"
    }
}
$executables = @('ascii-material-room.exe', 'ascii-engine-lab.exe', 'author.exe')
Assert-Regular $release
Assert-Regular $licenses
foreach ($name in $executables) {
    $path = Join-Path $release $name
    Assert-Regular $path
    Assert-Amd64 $path
}
foreach ($path in @((Join-Path $root 'LICENSE'), (Join-Path $licenses 'manifest.json'), (Join-Path $root 'docs/PORTABLE_PACKAGE.md'), (Join-Path $PSScriptRoot 'probe-material-room.ps1'))) {
    Assert-Regular $path
}
$licenseManifest = Get-Content -LiteralPath (Join-Path $licenses 'manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
if (@($licenseManifest).Count -eq 0) { throw 'Empty dependency license manifest' }
# The collector/reviewer owns the completeness decision; the packager verifies each listed notice exists.
foreach ($dependency in $licenseManifest) {
    if (@($dependency.files).Count -eq 0) { throw "No reviewed notice listed for $($dependency.name)-$($dependency.version)" }
    foreach ($notice in $dependency.files) {
        $relative = "$($dependency.name)-$($dependency.version)/$notice"
        $noticePath = [IO.Path]::GetFullPath((Join-Path $licenses $relative))
        if (!$noticePath.StartsWith($licenses.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'License notice escapes input directory' }
        Assert-Regular $noticePath
    }
}
$licenseFiles = @(Get-ChildItem -LiteralPath $licenses -Recurse -Force)
foreach ($item in $licenseFiles) { Assert-Regular $item.FullName }
$revision = (& git -C $root rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $revision -notmatch '^[a-f0-9]{40}$') { throw 'Cannot identify source revision' }
$dirty = @(& git -C $root status --porcelain --untracked-files=normal).Count -ne 0
if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect source status' }
$null = New-Item -ItemType Directory -Path $output
try {
    foreach ($name in $executables) { Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $output $name) }
    Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $output 'LICENSE')
    Copy-Item -LiteralPath (Join-Path $root 'docs/PORTABLE_PACKAGE.md') -Destination (Join-Path $output 'PORTABLE_PACKAGE.md')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'probe-material-room.ps1') -Destination (Join-Path $output 'probe-material-room.ps1')
    $licenseOutput = Join-Path $output 'third-party-licenses'
    $null = New-Item -ItemType Directory -Path $licenseOutput
    foreach ($item in $licenseFiles | Where-Object { !$_.PSIsContainer }) {
        $relative = $item.FullName.Substring($licenses.TrimEnd('\', '/').Length + 1)
        $destination = Join-Path $licenseOutput $relative
        $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination)
        Copy-Item -LiteralPath $item.FullName -Destination $destination
    }
    $null = New-Item -ItemType Directory -Path (Join-Path $output 'projects')
    # The destination project is self-contained; no repository paths are embedded.
    $author = Join-Path $output 'author.exe'
    $starter = Join-Path $output 'projects/starter.project.json'
    $oldPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $authorLog = @(& $author new $starter 2>&1)
    $authorExit = $LASTEXITCODE
    $ErrorActionPreference = $oldPreference
    if ($authorExit -ne 0 -or !(Test-Path -LiteralPath $starter)) { throw "author new failed (exit $authorExit). MSVC x64 runtime is required. $($authorLog -join [Environment]::NewLine)" }
    $starterJson = Get-Content -LiteralPath $starter -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($starterJson.version -ne 1 -or $starterJson.scene.version -ne 2 -or $starterJson.updates.version -ne 1) { throw 'Unexpected starter project schema' }
    Write-Utf8 (Join-Path $output 'Start-MaterialRoom.cmd') @'
@echo off
setlocal
pushd "%~dp0"
if not exist "work" mkdir "work"
if not exist "work\room.project.json" (
  "%~dp0author.exe" save-as "projects\starter.project.json" "work\room.project.json"
  if errorlevel 1 (
    popd
    exit /b 1
  )
)
"%~dp0ascii-material-room.exe" --project "work\room.project.json" --profile balanced --workers auto --output "results\interactive-room" %*
set "result=%errorlevel%"
popd
if not "%result%"=="0" echo Launch failed. Read PORTABLE_PACKAGE.md and run Probe.cmd for diagnostics.
exit /b %result%
'@
    Write-Utf8 (Join-Path $output 'Start-Laboratory.cmd') @'
@echo off
setlocal
pushd "%~dp0"
"%~dp0ascii-engine-lab.exe" --stage cloth --profile balanced --workers auto --output "results\interactive-lab" %*
set "result=%errorlevel%"
popd
if not "%result%"=="0" echo Launch failed. Read PORTABLE_PACKAGE.md and run Probe.cmd for diagnostics.
exit /b %result%
'@
    Write-Utf8 (Join-Path $output 'Probe.cmd') @'
@echo off
setlocal
powershell.exe -NoProfile -File "%~dp0probe-material-room.ps1" -PackageDirectory "%~dp0." %*
exit /b %errorlevel%
'@
    $files = @(Get-ChildItem -LiteralPath $output -Recurse -File -Force | Sort-Object FullName | ForEach-Object {
        [ordered]@{ path=$_.FullName.Substring($output.TrimEnd('\', '/').Length + 1).Replace('\','/'); bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $manifest = [ordered]@{
        schema_version=1; package='ascii-engine-lab-material-room'; target='x86_64-pc-windows-msvc'
        created_utc=[DateTime]::UtcNow.ToString('o')
        source=[ordered]@{ revision=$revision; working_tree_dirty=$dirty; provenance='Already-built inputs supplied by the release operator; packaging does not compile or attest a source/binary correspondence.' }
        prerequisites=@('Windows x64', 'Microsoft Visual C++ x64 runtime (VCRUNTIME140.dll) and Universal CRT', 'A driver/backend meeting the embedded wgpu requirements for GPU modes', 'Windows PowerShell 5.1 or compatible PowerShell for the probe')
        starter_project='projects/starter.project.json'; executables=$executables; files=$files
        limits=@('Relocation is not validation on a second physical host.', 'Hashes detect changes; this manifest is not a digital signature.', 'No runtime DLLs, driver installer or toolchain are bundled.', 'No compiler is invoked by this package. Binaries may retain compiler diagnostic source strings; runtime data paths are relative or explicitly supplied.')
    }
    Write-Utf8 (Join-Path $output 'package-manifest.json') (($manifest | ConvertTo-Json -Depth 10) + "`n")
    if (!$NoArchive) {
        Compress-Archive -LiteralPath @(Get-ChildItem -LiteralPath $output -Force | ForEach-Object { $_.FullName }) -DestinationPath $archive -CompressionLevel Optimal
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Write-Utf8 ($archive + '.sha256') ($hash + '  ' + [IO.Path]::GetFileName($archive) + "`n")
    }
    [ordered]@{ directory=$output; archive=$(if ($NoArchive) { $null } else { $archive }); payload_files=$files.Count; source_revision=$revision; source_dirty=$dirty; status='assembled; independent integrity check and runtime probe still required' } | ConvertTo-Json
} catch {
    # Preserve incomplete output for diagnosis; never delete or overwrite a prior package.
    throw "Package assembly failed; incomplete output retained at '$output'. $($_.Exception.Message)"
}
