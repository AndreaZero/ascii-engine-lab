# Release binaries required. Run alone: no builds, other measurements or encoder.
param([switch]$ClientOnly)
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path -Parent $PSScriptRoot
$out = Join-Path $labRoot 'evidence/r6'
$lab = Join-Path $labRoot 'target/release/ascii-engine-lab.exe'
$room = Join-Path $labRoot 'target/release/ascii-material-room.exe'
$checks = @()
Push-Location $labRoot
try {
    & $room --validate *> (Join-Path $out 'validate.log')
    if ($LASTEXITCODE -ne 0) { throw 'Default content validation failed' }
    foreach ($backend in @('vulkan','dx12')) {
        if (-not $ClientOnly) {
            & $lab --benchmark --frames 12 --profile balanced --workers 8 --backend $backend --output (Join-Path $out "lab-$backend") *> (Join-Path $out "lab-$backend.log")
            if ($LASTEXITCODE -ne 0) { throw "Lab failed: $backend" }
        }
        & $room --headless --camera all --frames 120 --profile balanced --workers 8 --backend $backend --output (Join-Path $out "room-$backend") *> (Join-Path $out "room-$backend.log")
        if ($LASTEXITCODE -ne 0) { throw "Room failed: $backend" }
        $checks += @{case=$backend; room_exit_code=0; lab_run_this_pass=(-not $ClientOnly)}
        Write-Output "${backend}: passed"
    }
    $source = Get-Content -Raw -LiteralPath 'clients/material-room/assets/material-room.json' | ConvertFrom-Json
    $source.name = 'Data edit: red textile, rotated stand and pale blue timber'
    $source.materials[1].pigment = @(0.4,0.63,0.69)
    $source.materials[2].pigment = @(0.77,0.18,0.11)
    $source.objects[5].rotation_y = -0.5
    $variant = Join-Path $out 'changed-scene.json'
    [System.IO.File]::WriteAllText($variant, ($source | ConvertTo-Json -Depth 12), (New-Object System.Text.UTF8Encoding($false)))
    & $room --scene $variant --headless --camera all --frames 120 --profile balanced --workers 8 --backend vulkan --output (Join-Path $out 'room-changed') *> (Join-Path $out 'room-changed.log')
    if ($LASTEXITCODE -ne 0) { throw 'Changed content failed' }
    $invalid = Join-Path $out 'invalid-scene.json'
    [System.IO.File]::WriteAllText($invalid, '{"version":999}', (New-Object System.Text.UTF8Encoding($false)))
    & $room --validate --scene $invalid *> (Join-Path $out 'invalid-scene.log')
    $invalidCode = $LASTEXITCODE
    if ($invalidCode -eq 0) { throw 'Invalid content incorrectly accepted' }
    $checks += @{case='invalid-scene'; exit_code=$invalidCode}
    # Hidden background helpers; both real window/surface event loops still run.
    foreach ($name in @('ascii-engine-lab','ascii-material-room')) {
        if ($ClientOnly -and $name -eq 'ascii-engine-lab') { continue }
        $p = Start-Process -FilePath (Join-Path $labRoot "target/release/$name.exe") -ArgumentList '--seconds','2','--backend','vulkan' -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput (Join-Path $out "$name-window.log") -RedirectStandardError (Join-Path $out "$name-window-error.log")
        if ($p.ExitCode -ne 0) { throw "$name window smoke failed" }
        $checks += @{case="$name-window"; exit_code=$p.ExitCode}
    }
    Write-Output 'Data edit, invalid input and both windows passed'
} finally {
    Pop-Location
    $resultName = if ($ClientOnly) { 'runtime-checks-client-final.json' } else { 'runtime-checks.json' }
    $checks | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out $resultName) -Encoding utf8
}
# The deliberately rejected JSON above returned a nonzero native exit code.
# Reaching this point means every expected outcome, including rejection, passed.
$global:LASTEXITCODE = 0
