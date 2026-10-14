@echo off
setlocal
cd /d "%~dp0"
cargo build --locked --release -p ascii-material-room
if errorlevel 1 goto failed
target\release\ascii-material-room.exe --scene clients/material-room/assets/coverage-room-v2.json --profile detail --sampling conservative9 --camera-path coverage --workers auto --output evidence/r12/interactive
if errorlevel 1 goto failed
exit /b 0
:failed
echo Build or execution failed. See the message above.
pause
exit /b 1
