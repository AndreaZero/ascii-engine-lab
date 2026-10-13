@echo off
cd /d "%~dp0"
if exist "target\release\ascii-engine-lab.exe" (
  "target\release\ascii-engine-lab.exe" %*
) else (
  cargo run --locked --release -- %*
)
if errorlevel 1 pause
