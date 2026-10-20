# Bot-pool + stream-list matrix against a running Rust port.
#
#   powershell -NoProfile -File port\rust\tools\e2e_matrix.ps1
#
# Prints one line per case: HTTP status, stream count and the first file name.
# Nothing here mutates the pool except a set-active round-trip (it switches to a
# non-active bot and immediately switches back), so the active session that
# streaming depends on is preserved.

param([string]$Base = 'http://127.0.0.1:8088')

$ErrorActionPreference = 'Continue'

function Get-Json([string]$path) {
    $raw = & curl.exe -s -m 60 ("{0}{1}" -f $Base, $path)
    try { return $raw | ConvertFrom-Json } catch { return $null }
}

function Post-Json([string]$path, [string]$body) {
    $raw = & curl.exe -s -m 60 -X POST -H 'Content-Type: application/json' -d $body ("{0}{1}" -f $Base, $path)
    try { return $raw | ConvertFrom-Json } catch { return $raw }
}

Write-Host '== BOT POOL =='
$bots = Get-Json '/api/bots'
if ($bots) {
    Write-Host ("total={0} active={1}" -f $bots.total_bots, $bots.active_bot_id)
    foreach ($b in $bots.bots) {
        Write-Host ("  {0}  {1,-34} session={2} active={3} status={4}" -f $b.bot_id, $b.bot_username, $b.has_session, $b.is_active, $b.status)
    }
}
$r = Post-Json '/api/bots/add' '{}'
Write-Host ("  add(empty)        -> {0}" -f $r.message)
$r = Post-Json '/api/bots/set-active' '{"bot_id":"0"}'
Write-Host ("  set-active(0)     -> {0}" -f $r.message)
$r = Post-Json '/api/bots/remove' '{"bot_id":""}'
Write-Host ("  remove(empty)     -> {0}" -f $r.message)

$other = ($bots.bots | Where-Object { -not $_.is_active } | Select-Object -First 1)
if ($other) {
    $r = Post-Json '/api/bots/set-active' ("{`"bot_id`":`"{0}`"}" -f $other.bot_id)
    Write-Host ("  set-active({0}) -> ok={1}" -f $other.bot_id, $r.ok)
    $r = Post-Json '/api/bots/set-active' ("{`"bot_id`":`"{0}`"}" -f $bots.active_bot_id)
    Write-Host ("  set-active(back)   -> ok={0}" -f $r.ok)
}

Write-Host ''
Write-Host '== STREAM LISTS =='
$cases = @(
    @('movie', 'pm:post:9000031194'),
    @('movie', 'pm:post:8591885201'),
    @('movie', 'pm:post:9001801064'),
    @('series', 'pm:post:9000016669:1:1'),
    @('series', 'tt11540908:1:1'),
    @('movie', 'tt34384900'),
    @('movie', 'tt27369017'),
    @('movie', 'pm_file_1v9OPj'),
    @('movie', 'tt99999999')
)
foreach ($c in $cases) {
    $type = $c[0]
    $id = $c[1]
    $enc = [uri]::EscapeDataString($id)
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $j = Get-Json ("/stream/$type/$enc.json")
    $sw.Stop()
    $n = if ($j) { @($j.streams).Count } else { -1 }
    $first = ''
    if ($n -gt 0) {
        $desc = [string]$j.streams[0].description
        $first = ($desc -split "`n")[-1]
    }
    Write-Host ("{0,-26} {1,4} streams {2,7:N2}s  {3}" -f $id, $n, $sw.Elapsed.TotalSeconds, $first)
}
