# Stage the Windows release of the server.
#
#   powershell -NoProfile -File port\rust\tools\build-dist.ps1
#
# The whole server is ONE executable: the dashboard (`public/`) is compiled into
# the binary with `include_dir!`, so nothing has to sit beside it — no `public/`
# folder, no PHP, no Composer, no `vendor/`.
#
# Output: port\rust\dist\
#   pencarimovie-server.exe   the server (dashboard embedded)
#   storage\                  empty state directory (sessions, auth.json, caches)
#   start.bat                 console launcher
#   start-tray.bat            hidden start + tray icon
#   tray.ps1                  the tray (open pages, restart, stop, run at login)
#   autostart-on.bat/.off     login entry (HKCU Run)
#   pencarimovie.ico          logo used by the tray
#   README.txt                what this is
#
# `storage/` is state, not code: keep it across upgrades.

param(
  [string]$Configuration = 'release',
  [string]$OutDir = $null
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repo = (Resolve-Path (Join-Path $root '..\..')).Path
if (-not $OutDir) { $OutDir = Join-Path $root 'dist' }

$exe = Join-Path $root "target\$Configuration\pencarimovie-server.exe"
if (-not (Test-Path $exe)) {
  throw "missing $exe - run: cargo build --profile $Configuration"
}

if (Test-Path $OutDir) { Remove-Item $OutDir -Recurse -Force }
New-Item -ItemType Directory -Path $OutDir | Out-Null
New-Item -ItemType Directory -Path (Join-Path $OutDir 'storage') | Out-Null
Copy-Item $exe (Join-Path $OutDir 'pencarimovie-server.exe')

# Optional desktop helpers (tray, launchers, login entry) + the product logo.
$desk = Join-Path $root 'desktop'
Copy-Item (Join-Path $desk 'windows\*') $OutDir
Copy-Item (Join-Path $desk 'README.txt') $OutDir
$ico = Join-Path $repo 'tray.ico'
if (Test-Path $ico) {
  Copy-Item $ico (Join-Path $OutDir 'pencarimovie.ico')
}
else {
  Write-Warning "tray.ico not found at $ico - the tray will use a stock icon"
}

$size = (Get-Item (Join-Path $OutDir 'pencarimovie-server.exe')).Length
Write-Host ("dist ready: {0}" -f $OutDir)
Write-Host ("  exe: {0:N0} bytes (dashboard embedded)" -f $size)
Write-Host '  try: .\pencarimovie-server.exe --port 8093   then  curl -sI http://127.0.0.1:8093/'
Write-Host '  tray: .\start-tray.bat   (hidden + icon)   login: .\autostart-on.bat'
