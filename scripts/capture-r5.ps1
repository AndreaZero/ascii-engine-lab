param([int]$Frames = 180, [int]$Workers = 8, [string]$Out = 'evidence/r5/motion')
$ErrorActionPreference = 'Stop'
$labRoot = Split-Path -Parent $PSScriptRoot
Push-Location $labRoot
try {
    if (Test-Path -LiteralPath $Out) { throw "Output already exists: $Out. Choose a new destination." }
    foreach ($variant in @('baseline','current')) {
        & .\target\release\examples\character_study.exe --camera all --variant $variant --frames $Frames --fps 30 --workers $Workers --backend vulkan --out $Out
        if ($LASTEXITCODE -ne 0) { throw "Capture failed: $variant" }
    }
} finally { Pop-Location }
