@echo off
cd /d "%~dp0"
if not exist "target\release\examples\environment_study.exe" (
    cargo build --locked --release --example environment_study
    if errorlevel 1 (
        pause
        exit /b 1
    )
)
"target\release\examples\environment_study.exe" --window --variant on --camera third %*
