param([int]$Frames = 120, [string]$Out = 'evidence/r5/cpu')
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path -Parent $PSScriptRoot
Push-Location $labRoot
try {
    if (Test-Path -LiteralPath $Out) { throw "Output already exists: $Out. Choose a new destination." }
    $null = New-Item -ItemType Directory -Path $Out -Force
    $order = @()
    foreach ($rep in 1..3) {
        $budgets = if ($rep % 2 -eq 1) { @(1,8) } else { @(8,1) }
        foreach ($workers in $budgets) {
            $variants = if ($rep % 2 -eq 1) { @('baseline','current') } else { @('current','baseline') }
            foreach ($variant in $variants) {
                $destination = Join-Path $Out "rep$rep-w$workers"
                $order += [ordered]@{ repetition=$rep; workers=$workers; variant=$variant; destination=$destination }
                & .\target\release\examples\character_study.exe --benchmark --camera all --variant $variant --frames $Frames --fps 30 --workers $workers --out $destination
                if ($LASTEXITCODE -ne 0) { throw "CPU study failed: $variant/$workers/$rep" }
            }
        }
    }
    [ordered]@{
        executable_sha256=(Get-FileHash -LiteralPath target/release/examples/character_study.exe -Algorithm SHA256).Hash.ToLower()
        order=$order
        scope='Same executable, intentional content and history changes; NOT equivalent-quality optimisation. Separate process per variant; order alternates by repetition. Composition-only CPU timer, snapshot separate. Untimed counts between frames; desktop not isolated. No other lab build/encode/benchmark running concurrently.'
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $Out 'protocol.json') -Encoding utf8
} finally { Pop-Location }
