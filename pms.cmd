@echo off
rem ── PencariMovie Server launcher (NATIVE build only) ─────────────────────────
rem One binary, no PHP and no OTA zip: this only starts pencarimovie-server.exe.
rem Search order: next to this file (extracted package / git checkout build),
rem then the per-user install written by `irm telegra.my/win | iex`, then
rem port\rust\target\release for a local cargo build.
setlocal

set "PM_USER_DIR=%USERPROFILE%\pencarimovie-server"

if exist "%~dp0pencarimovie-server.exe" (
    set "PM_DIR=%~dp0."
    goto run
)
if exist "%PM_USER_DIR%\pencarimovie-server.exe" (
    set "PM_DIR=%PM_USER_DIR%"
    goto run
)
if exist "%~dp0port\rust\target\release\pencarimovie-server.exe" (
    set "PM_DIR=%~dp0port\rust\target\release"
    goto run
)

echo PencariMovie Server is not installed.
echo   Install:   irm telegra.my/win ^| iex
echo   Releases:  https://github.com/aiskendi/pencarimovie-server/releases/latest
exit /b 1

:run
rem NO ARGUMENTS -> tray (icon in the notification area) like the installer does.
if "%~1"=="" (
    if exist "%PM_DIR%\start-tray.bat" (
        pushd "%PM_DIR%"
        call start-tray.bat
        popd
        exit /b 0
    )
)
pushd "%PM_DIR%"
"pencarimovie-server.exe" %*
set "RC=%ERRORLEVEL%"
popd
exit /b %RC%
