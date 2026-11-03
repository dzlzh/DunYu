@echo off
setlocal
cd /d "%~dp0"
if not exist "dunyu.exe" (
  echo Place this script next to the diagnostic dunyu.exe and your existing data folder.
  pause
  exit /b 1
)
echo Exit the running DunYu from its tray menu first.
echo First open settings and focus the hotkey field. Hold Ctrl+Alt, then release.
echo Hold left Win, press Q, and release. Save if recording succeeded.
echo Then test physical Win+Q with Desktop, Windows Terminal and WezTerm in the foreground.
echo Return to each test window before every FIRST press. If Search opens, press again.
echo Fully release both Win and Q after every press.
echo Also test again after summoning and hiding Telegram back to the terminal.
echo Repeat five times per application. Exit DunYu normally when finished.
echo Expected revision: winq-trace-v4; observers.raw_input=false and observers.foreground=false.
echo Default tracing does not register either extra observer.
echo Logs: data\hotkey-diagnostics-*.jsonl
start "" /wait "%~dp0dunyu.exe" --diagnose-hotkey=Win+Q
echo Session ended. Check the log for session_end and complete=true.
echo Log completeness only confirms capture integrity, not recording or takeover success.
pause
