@echo off
setlocal
cd /d "%~dp0"
cargo build --locked --release --example visual_capacity
if errorlevel 1 exit /b %errorlevel%
target\release\examples\visual_capacity.exe --window --figure warden --surface structured --camera first --distance near
endlocal
