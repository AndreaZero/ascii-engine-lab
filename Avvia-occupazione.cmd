@echo off
setlocal
cd /d "%~dp0"
cargo build --locked --release -p ascii-material-room
if errorlevel 1 goto failed
target\release\ascii-material-room.exe --scene clients/material-room/assets/dynamic-room-v2.json --updates clients/material-room/assets/dynamic-updates-v1.json --profile detail --workers auto --output evidence/r14/interactive %*
if errorlevel 1 goto failed
exit /b 0
:failed
echo Build or execution failed. See the message above.
pause
exit /b 1
