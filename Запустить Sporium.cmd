@echo off
setlocal
set "SPORIUM_TEST_DATA_DIR="
set "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="
set "SPORIUM_EXE=%~dp0src-tauri\target\debug\sporium.exe"
if not exist "%SPORIUM_EXE%" (
  echo Sporium.exe was not found. Build the project with npm.cmd run desktop:build.
  pause
  exit /b 1
)
start "Sporium" "%SPORIUM_EXE%"
exit /b 0
