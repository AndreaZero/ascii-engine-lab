# Release build first. Run after CPU measurements; never alongside them.
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path -Parent $PSScriptRoot
$out = Join-Path $labRoot 'evidence/r4'
$null = New-Item -ItemType Directory -Path $out -Force
$binary = Join-Path $labRoot 'target/release/ascii-engine-lab.exe'
$results = @()
Push-Location $labRoot
try {
    foreach ($backend in @('vulkan','dx12')) {
        $destination = Join-Path $out ('gpu-' + $backend)
        & $binary --benchmark --frames 12 --profile balanced --workers 8 --backend $backend --output $destination *> (Join-Path $out ($backend + '.log'))
        $code = $LASTEXITCODE
        $results += [ordered]@{case=$backend; exit_code=$code; path=$destination}
        if ($code -ne 0) { throw "Failed R4 renderer smoke: $backend" }
        Write-Output "$backend passed"
    }
    & $binary --seconds 2 --backend vulkan *> (Join-Path $out 'window.log')
    $code = $LASTEXITCODE
    $results += [ordered]@{case='window-vulkan'; exit_code=$code}
    if ($code -ne 0) { throw 'Failed R4 window smoke' }
    Write-Output 'Window passed'
} finally {
    Pop-Location
    $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $out 'runtime-checks.json') -Encoding utf8
}
