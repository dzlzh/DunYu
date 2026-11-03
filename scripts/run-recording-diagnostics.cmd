@echo off
setlocal
cd /d "%~dp0"
echo Exit DunYu first. Release all keys and keep the test window visible.
echo The test uses temporary settings. Do not type or switch windows during the test.
pause
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-recording-diagnostics.ps1"
set "result=%errorlevel%"
pause
exit /b %result%
