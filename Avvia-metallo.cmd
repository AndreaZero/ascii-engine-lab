@echo off
cd /d "%~dp0"
if not exist "target\release\examples\metal_study.exe" (
    cargo build --locked --release --example metal_study
    if errorlevel 1 (
        pause
        exit /b 1
    )
)
"target\release\examples\metal_study.exe" --window --variant forged --camera third %*
