@echo off
rem ── PencariMovie Server launcher (native) ───────────────────────────────────
rem Starts pencarimovie-server.exe from THIS folder. With no arguments it runs
rem start-tray.bat, so the icon appears in the notification area; any other
rem argument is passed straight to the binary (--port, --version, --no-open, ...).
setlocal

if not exist "%~dp0pencarimovie-server.exe" (
    echo pencarimovie-server.exe is missing next to this file.
    echo Reinstall the package, or run:  irm telegra.my/win ^| iex
    exit /b 1
)

if "%~1"=="" (
    if exist "%~dp0start-tray.bat" (
        call "%~dp0start-tray.bat"
        exit /b 0
    )
)

pushd "%~dp0."
"%~dp0pencarimovie-server.exe" %*
set "RC=%ERRORLEVEL%"
popd
exit /b %RC%
