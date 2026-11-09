@echo off
setlocal
if exist "%~dp0dist\Papyrus.exe" (
  start "" "%~dp0dist\Papyrus.exe"
  exit /b 0
)
if exist "%~dp0target\release\papyrus.exe" (
  start "" "%~dp0target\release\papyrus.exe"
  exit /b 0
)
echo Run cargo build --release --locked in this folder first.
echo Microsoft Edge WebView2 Evergreen Runtime is required.
pause
exit /b 1
