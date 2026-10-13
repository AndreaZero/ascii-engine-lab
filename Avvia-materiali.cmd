@echo off
cd /d "%~dp0"
if not exist "target\release\ascii-material-room.exe" (
    cargo build --locked --release -p ascii-material-room
    if errorlevel 1 (
        pause
        exit /b 1
    )
)
"target\release\ascii-material-room.exe" --scene "clients\material-room\assets\material-room.json" %*
