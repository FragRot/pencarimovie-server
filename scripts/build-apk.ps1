# Run a long build WITHOUT hanging the terminal, with a live indicator.
#
#   powershell -NoProfile -File scripts\build-apk.ps1                # native APK
#   powershell -NoProfile -File scripts\build-apk.ps1 -Args --skip-rust
#   powershell -NoProfile -File scripts\build-apk.ps1 -Status         # just show progress of the last run
#
# Why this exists: `scripts\build-rust-apk.bat` takes minutes (4 Android ABIs of
# the Rust engine + gradle). Chaining it with `&` in cmd makes the terminal look
# frozen with no output, and a tool that kills the chain kills the build with it.
# This wrapper starts the build DETACHED, tails its log, and prints one line per
# poll (stage, elapsed, last log line), so you always see progress and the build
# survives the terminal being closed.

param(
    [string]$LogFile = "$PSScriptRoot\..\_apk_build.log",
    [string[]]$Args = @(),
    [int]$TimeoutSec = 3600,
    [switch]$Status
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$bat = Join-Path $repo 'scripts\build-rust-apk.bat'

function Get-Stage([string]$line) {
    if ($line -match 'building\s+(\S+)') { return "dist-native $($Matches[1])" }
    if ($line -match '^\s*-->\s*(.+)$') { return "cargo $($Matches[1])" }
    if ($line -match 'BUILD SUCCESSFUL') { return 'gradle done' }
    if ($line -match 'staged|sha256') { return 'staging APKs' }
    if ($line -match 'artifact:') { return 'packaging' }
    return 'working'
}

function Write-ProgressLine($started, [switch]$Final) {
    if (-not (Test-Path $LogFile)) { return }
    $lines = @(Get-Content $LogFile -Tail 400 -ErrorAction SilentlyContinue)
    if (-not $lines.Count) { return }
    $last = ($lines | Where-Object { $_.Trim() } | Select-Object -Last 1)
    $stage = Get-Stage ($lines -join "`n")
    $elapsed = [int]((Get-Date) - $started).TotalSeconds
    $mm = [int]($elapsed / 60); $ss = $elapsed % 60
    $tag = if ($Final) { 'done' } else { 'run ' }
    Write-Host ("[$tag] {0,3}:{1:00}  {2,-16} {3}" -f $mm, $ss, $stage, ($last -replace '\s+', ' ').Trim())
}

if ($Status) {
    # Show where the last run got to, then exit (never blocks).
    if (Test-Path $LogFile) { Write-ProgressLine (Get-Item $LogFile).LastWriteTime -Final } else { Write-Host "no log at $LogFile" }
    exit 0
}

$started = Get-Date
Write-Host "build : $bat $($Args -join ' ')"
Write-Host "log   : $LogFile"
Write-Host "       (this window prints one line every 5 s; the build keeps running if you close it)"

# Detached: the child is not killed when this shell ends, and nothing blocks.
Start-Process -FilePath 'cmd.exe' `
    -ArgumentList (@('/c', "`"$bat`"") + $Args + @('>', "`"$LogFile`"", '2>&1')) `
    -WorkingDirectory $repo -WindowStyle Hidden | Out-Null

$deadline = (Get-Date).AddSeconds($TimeoutSec)
$lastSize = -1
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 5
    Write-ProgressLine $started
    $size = if (Test-Path $LogFile) { (Get-Item $LogFile).Length } else { 0 }
    # A finished build stops growing AND has its staging summary in the tail.
    $tail = if (Test-Path $LogFile) { (Get-Content $LogFile -Tail 5) -join "`n" } else { '' }
    if ($tail -match 'done\. APKs in|BUILD SUCCESSFUL' -and $size -eq $lastSize -and $size -gt 0) {
        Write-ProgressLine $started -Final
        Get-Content $LogFile -Tail 14
        Write-Host "OK - APKs are in android\_apk_dist"
        exit 0
    }
    $lastSize = $size
}
Write-Host "TIMEOUT after $TimeoutSec s - the build may still be running; log: $LogFile"
exit 1
