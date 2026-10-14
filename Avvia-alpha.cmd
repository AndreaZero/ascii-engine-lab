@echo off
cd /d "%~dp0"
rem Build every time so a previous release executable cannot hide source edits.
cargo build --locked --release -p ascii-material-room
if errorlevel 1 (
    pause
    exit /b 1
)
"target\release\ascii-material-room.exe" --scene "clients\material-room\assets\material-room-v2.json" %*
