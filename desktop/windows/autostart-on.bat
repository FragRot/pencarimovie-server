@echo off
rem Start PencariMovie Server (tray + server) when this user logs in.
rem Registers a per-user HKCU Run entry - no administrator rights needed.
setlocal
cd /d "%~dp0"
if "%PORT%"=="" set PORT=8088
set "RUNKEY=HKCU\Software\Microsoft\Windows\CurrentVersion\Run"
set "NAME=PencariMovie Server"
set "CMD=powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"%~dp0tray.ps1\" -StartServer -Port %PORT%"
reg add "%RUNKEY%" /v "%NAME%" /t REG_SZ /d "%CMD%" /f >nul
if errorlevel 1 (
  echo Could not register the login entry.
  exit /b 1
)
echo Registered: "%NAME%" will start at login.
echo   command: %CMD%
