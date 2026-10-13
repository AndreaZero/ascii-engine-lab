# R3 compatibility smoke, not a hardware performance benchmark.
# Run after a release build. No build or other benchmark runs in parallel.
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $labRoot 'target/release/ascii-engine-lab.exe'
$evidenceRoot = Join-Path $labRoot 'evidence/r3'
$null = New-Item -ItemType Directory -Force -Path $evidenceRoot
$cases = @(
    @{ name='dx12'; args=@('--benchmark','--frames','30','--backend','dx12','--workers','2'); expect=0 },
    @{ name='vulkan'; args=@('--benchmark','--frames','30','--backend','vulkan','--workers','2'); expect=0 },
    @{ name='economy'; args=@('--headless','--profile','economy'); expect=0 },
    @{ name='detail'; args=@('--headless','--profile','detail'); expect=0 },
    @{ name='fallback-dx12'; args=@('--headless','--backend','dx12','--fallback-adapter','--width','640','--height','360','--cols','80','--rows','45','--workers','1'); expect=$null },
    @{ name='unsupported-metal'; args=@('--diagnostics','--backend','metal'); expect=1 },
    @{ name='window-vulkan'; args=@('--seconds','2','--backend','vulkan'); expect=0 },
    @{ name='window-dx12'; args=@('--seconds','2','--backend','dx12'); expect=0 }
)
$records = @()
Push-Location $labRoot
try {
    foreach ($case in $cases) {
        $arguments = $case.args + @('--output', (Join-Path $evidenceRoot $case.name))
        $log = Join-Path $evidenceRoot ($case.name + '.log')
        $started = Get-Date
        # Expected error is evidence too. Native exit code is collected below.
        $oldPreference = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $binary @arguments *> $log
        $code = $LASTEXITCODE
        $ErrorActionPreference = $oldPreference
        $records += [ordered]@{ case=$case.name; arguments=$arguments; exit_code=$code; expected_exit=$case.expect; started=$started.ToString('o'); elapsed_s=((Get-Date)-$started).TotalSeconds; log=$log }
        Write-Output ($case.name + ': exit ' + $code)
    }
} finally {
    Pop-Location
}
$records | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceRoot 'runtime-checks.json') -Encoding utf8
$failed = @($records | Where-Object { $null -ne $_.expected_exit -and $_.exit_code -ne $_.expected_exit })
if ($failed.Count -gt 0) { throw "Unexpected results in runtime-checks.json: $($failed.case -join ', ')" }
