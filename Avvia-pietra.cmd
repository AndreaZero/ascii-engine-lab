@echo off
cd /d "%~dp0"
if not exist "target\release\examples\stone_study.exe" (
    cargo build --locked --release --example stone_study
    if errorlevel 1 (
        pause
        exit /b 1
    )
)
"target\release\examples\stone_study.exe" --window --variant dry --camera third %*
