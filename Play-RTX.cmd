@echo off
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Play-RTX.ps1" %*
exit /b %ERRORLEVEL%
