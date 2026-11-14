@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0register-context-menu.ps1"
set "ryne_result=%errorlevel%"
pause
exit /b %ryne_result%
