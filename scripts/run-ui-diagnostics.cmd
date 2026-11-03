@echo off
setlocal
cd /d "%~dp0"
echo Ready to START diagnostics. Press a key once to begin.
echo Keep test windows visible and unobscured. Do not interact during testing.
echo Navigation verification runs three times. A/B benchmarks and image saving can take 3-6 minutes.
pause
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0run-ui-diagnostics.ps1"
set "diagnostic_exit=%errorlevel%"
if not "%diagnostic_exit%"=="0" (
    echo Diagnostics FAILED with exit code %diagnostic_exit%. Keep all logs and any rest-pixel-diagnostics-* folder for analysis.
) else (
    echo Diagnostics COMPLETED. Send diagnostic-run.log, benchmark-*.json, performance-comparison.json and the newest ui-diagnostics-* folder.
)
echo Press a key to CLOSE this window.
pause
exit /b %diagnostic_exit%
