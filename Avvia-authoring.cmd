@echo off
setlocal
pushd "%~dp0"
cargo build --locked --release -p ascii-material-room --bins
if errorlevel 1 goto failed
if not exist "evidence\authoring" mkdir "evidence\authoring"
if not exist "evidence\authoring\room.project.json" (
  "target\release\author.exe" new "evidence\authoring\room.project.json"
  if errorlevel 1 goto failed
)
"target\release\ascii-material-room.exe" --project "evidence\authoring\room.project.json" --profile detail --workers auto --output "evidence\authoring\preview" %*
set "result=%errorlevel%"
popd
exit /b %result%
:failed
popd
exit /b 1
