# Live verification of the Rust port's /manifest.json against backend.php's
# descriptor contract (backend.php:15303-15489).
#
# Asserts:
#   1. `resources` is 4 OBJECTS (catalog only when a row survives, then
#      meta/stream/subtitles) and each carries `types`;
#   2. `types` follows `enabled_types` (movie off -> series,other);
#   3. a country catalog's `genre` extra is the 69-option combined list
#      (47 years first, then 22 genres);
#   4. `pm_topkw_*` rows carry NO `extraSupported` / `extraRequired`;
#   5. `description` names the address the addon was installed from;
#   6. `ETag` is sent and a matching `If-None-Match` answers 304.

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$app = Join-Path $repo 'port\rust'
$exe = Join-Path $app 'target\debug\pencarimovie-server.exe'
$root = Join-Path $app '_e2e_manifest'
$storage = Join-Path $root 'storage'
New-Item -ItemType Directory -Force -Path $storage | Out-Null
$port = 18099
$base = "http://127.0.0.1:$port"

if (-not (Test-Path $exe)) { throw "build first: $exe is missing" }

$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-Settings([string]$json) {
    [IO.File]::WriteAllText((Join-Path $storage 'catalog_settings.json'), $json, $utf8)
}

$env:PM_APP_ROOT = $root
$proc = Start-Process -FilePath $exe -ArgumentList @('--port', "$port", '--host', '127.0.0.1') `
    -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $root 'out.log') `
    -RedirectStandardError (Join-Path $root 'err.log')

try {
    $up = $false
    for ($i = 0; $i -lt 80; $i++) {
        Start-Sleep -Milliseconds 250
        try {
            $r = Invoke-WebRequest -Uri "$base/api/version" -TimeoutSec 2 -UseBasicParsing
            if ($r.StatusCode -eq 200) { $up = $true; break }
        }
        catch { }
        if ($proc.HasExited) { throw "server exited: $(Get-Content (Join-Path $root 'err.log') -Raw)" }
    }
    if (-not $up) { throw 'server did not start' }

    $fail = 0
    function Check($name, $cond, $detail) {
        if ($cond) { Write-Host "PASS  $name" }
        else { Write-Host "FAIL  $name  $detail"; $script:fail++ }
    }

    # ── case 1: defaults (no upstream, so the check is offline-deterministic) ──
    Write-Settings '{"catalogs_enabled":true,"enabled_types":{"movie":true,"series":true,"other":true},"upstream_enabled":false,"enabled_catalogs":{}}'
    $resp = Invoke-WebRequest -Uri "$base/manifest.json" -TimeoutSec 60 -UseBasicParsing
    $m = $resp.Content | ConvertFrom-Json
    $etag = $resp.Headers['ETag']
    Check 'HTTP 200' ($resp.StatusCode -eq 200) $resp.StatusCode
    Check 'ETag present' (-not [string]::IsNullOrWhiteSpace($etag)) $etag
    Check 'Cache-Control' ($resp.Headers['Cache-Control'] -eq 'max-age=1800, stale-while-revalidate=3600') $resp.Headers['Cache-Control']
    $resNames = @($m.resources | ForEach-Object { $_.name }) -join ','
    Check 'resources are 4 objects' ($resNames -eq 'catalog,meta,stream,subtitles') $resNames
    Check 'resources[0] is catalog w/ types' ($m.resources[0].name -eq 'catalog' -and @($m.resources[0].types).Count -gt 0) ''
    Check 'meta resource carries idPrefixes' (@($m.resources[1].idPrefixes).Count -eq 9) (@($m.resources[1].idPrefixes) -join '|')
    Check 'types = movie,series,other' ((@($m.types) -join ',') -eq 'movie,series,other') (@($m.types) -join ',')
    Check 'top-level idPrefixes order' ((@($m.idPrefixes) -join ',') -eq 'pm_,pm:,tt,tmdb:,kitsu:,kitsu,mal:,anilist:,tvdb:') (@($m.idPrefixes) -join ',')
    Check 'description names the address' ($m.description -match [regex]::Escape($base)) $m.description
    Check 'logo is the shipped asset' ($m.logo -eq "$base/logo.png") $m.logo
    Check 'behaviorHints.configurable' ($m.behaviorHints.configurable -eq $true) ''

    $country = @($m.catalogs | Where-Object { $_.id -eq 'pm_movies_malay' })[0]
    $opts = @($country.extra | Where-Object { $_.name -eq 'genre' })[0].options
    Write-Host ("      country genre options = {0}" -f @($opts).Count)
    Check 'country genre options = 47 years + 22 genres' (@($opts).Count -eq 69) (@($opts).Count)
    Check 'country options start with the current year' ((@($opts)[0] -eq (Get-Date).Year.ToString())) (@($opts)[0])
    Check 'country options end with Western' ((@($opts)[-1] -eq 'Western')) (@($opts)[-1])

    $yearRow = @($m.catalogs | Where-Object { $_.id -eq 'year' })[0]
    $yearOpts = @($yearRow.extra | Where-Object { $_.name -eq 'genre' })[0].options
    Check 'year row genre extra = year list' (@($yearOpts).Count -eq 47) (@($yearOpts).Count)

    $topkw = @($m.catalogs | Where-Object { $_.id -like 'pm_topkw_*' })
    if ($topkw.Count -gt 0) {
        Write-Host ("      topkw rows = {0} (first: {1})" -f $topkw.Count, $topkw[0].id)
        Check 'topkw rows have NO extraSupported' (@($topkw | Where-Object { $null -ne $_.extraSupported }).Count -eq 0) ''
        Check 'topkw rows have NO extraRequired' (@($topkw | Where-Object { $null -ne $_.extraRequired }).Count -eq 0) ''
        $kwGenre = @($topkw[0].extra | Where-Object { $_.name -eq 'genre' })[0].options
        Check 'topkw genre = 6 quality pills' ((@($kwGenre) -join ',') -eq '4K,1080p,720p,BluRay,WEB-DL,HEVC') (@($kwGenre) -join ',')
    }
    else {
        Write-Host '      topkw rows = 0 (trending keyword map empty or origin unreachable)'
    }

    # 304 on a matching If-None-Match
    $nm = $null
    try {
        $nm = Invoke-WebRequest -Uri "$base/manifest.json" -Headers @{ 'If-None-Match' = $etag } -TimeoutSec 30 -UseBasicParsing
        $code = $nm.StatusCode
    }
    catch {
        $code = [int]$_.Exception.Response.StatusCode
    }
    Check 'If-None-Match -> 304' ($code -eq 304) $code

    # ── case 2: movie disabled ────────────────────────────────────────────────
    Write-Settings '{"catalogs_enabled":true,"enabled_types":{"movie":false,"series":true,"other":true},"upstream_enabled":false,"enabled_catalogs":{}}'
    $m2 = (Invoke-WebRequest -Uri "$base/manifest.json" -TimeoutSec 60 -UseBasicParsing).Content | ConvertFrom-Json
    Check 'types = series,other when movie is off' ((@($m2.types) -join ',') -eq 'series,other') (@($m2.types) -join ',')
    $movieRows = @($m2.catalogs | Where-Object { $_.type -eq 'movie' })
    Check 'no movie catalogs advertised' ($movieRows.Count -eq 0) $movieRows.Count
    $resTypes = @($m2.resources[0].types) -join ','
    Check 'catalog resource types follow enabled_types' ($resTypes -eq 'series,other') $resTypes

    # ── case 3: catalogs off ──────────────────────────────────────────────────
    Write-Settings '{"catalogs_enabled":false,"enabled_types":{"movie":true,"series":true,"other":true},"upstream_enabled":false,"enabled_catalogs":{}}'
    $m3 = (Invoke-WebRequest -Uri "$base/manifest.json" -TimeoutSec 60 -UseBasicParsing).Content | ConvertFrom-Json
    Check 'catalogs off -> zero catalogs' (@($m3.catalogs).Count -eq 0) (@($m3.catalogs).Count)
    $names3 = @($m3.resources | ForEach-Object { $_.name }) -join ','
    Check 'catalogs off -> no catalog resource' ($names3 -eq 'meta,stream,subtitles') $names3

    # ── case 4: a forced identity mode ───────────────────────────────────────
    $m4 = (Invoke-WebRequest -Uri "$base/manifest.json?mode=lan" -TimeoutSec 60 -UseBasicParsing).Content | ConvertFrom-Json
    Check '?mode=lan -> addon.lan id' ($m4.id -eq 'org.pencarimovie.addon.lan') $m4.id
    Check '?mode=lan description mentions the LAN' ($m4.description -match 'LAN') $m4.description

    # ── case 5: Eclipse music manifest (backend.php:14315-14346) ─────────────
    $e = (Invoke-WebRequest -Uri "$base/eclipse/manifest.json" -TimeoutSec 30 -UseBasicParsing).Content | ConvertFrom-Json
    Check 'eclipse id' ($e.id -eq 'org.pencarimovie.eclipsemusic') $e.id
    Check 'eclipse types' ((@($e.types) -join ',') -eq 'track,album,artist') (@($e.types) -join ',')
    Check 'eclipse latest catalog row' (@($e.catalogs).Count -eq 1 -and $e.catalogs[0].id -eq 'latest') (@($e.catalogs).Count)
    Check 'eclipse quality setting' ($e.settings[0].key -eq 'quality') (@($e.settings).Count)

    Write-Host ''
    if ($fail -eq 0) { Write-Host 'RESULT: PASS - every descriptor assertion held' }
    else { Write-Host "RESULT: FAIL - $fail assertion(s) failed"; exit 1 }
}
finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
}
