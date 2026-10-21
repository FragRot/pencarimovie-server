@echo off
rem PencariMovie Server - system tray launcher (Windows).
rem
rem No console window: the server runs hidden and a tray icon controls it
rem (double-click = open dashboard, right-click = menu).
rem The same command is what "Run at login" registers.
setlocal
cd /d "%~dp0"
if "%PORT%"=="" set PORT=8088
start "" powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0tray.ps1" -StartServer -Port %PORT%
