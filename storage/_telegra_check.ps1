# Verify the deployed telegra.my worker: dark page, no ABI text, Rust links,
# and that every asset those links point at actually exists on releases/latest.
$ErrorActionPreference = 'Continue'

function Get-Status($u) {
    try {
        $r = Invoke-WebRequest -UseBasicParsing $u -TimeoutSec 40 -MaximumRedirection 5
        return [int]$r.StatusCode
    }
    catch {
        return 'ERR ' + $_.Exception.Message.Split([char]10)[0]
    }
}

'--- telegra.my ---'
try {
    # `$home` is a read-only automatic variable, so the page goes in `$page`.
    $page = Invoke-WebRequest -UseBasicParsing 'https://telegra.my/' -TimeoutSec 40
    'home          : ' + [int]$page.StatusCode + '  ' + $page.Content.Length + ' B'
    'dark theme    : ' + ($page.Content -match '#0d1117')
    'no arm64-v8a  : ' + (-not ($page.Content -match 'arm64-v8a'))
    'linux command : ' + ($page.Content -match 'telegra\.my/linux')
}
catch { 'home ERR: ' + $_.Exception.Message }

foreach ($p in @('/linux', '/win', '/mac', '/termux', '/docker', '/apk')) {
    try {
        $r = Invoke-WebRequest -UseBasicParsing ('https://telegra.my' + $p) -TimeoutSec 40
        '{0,-8} : {1}  {2,6} B  rust-ref={3}' -f $p, [int]$r.StatusCode, $r.Content.Length, ($r.Content -match 'pencarimovie-server')
    }
    catch {
        # /apk is a redirect to a large file; treat a redirect as fine.
        '{0,-8} : {1}' -f $p, $_.Exception.Message.Split([char]10)[0]
    }
}

'--- assets behind releases/latest ---'
foreach ($a in @(
        'pencarimovie-server-linux-x86_64.tar.gz',
        'pencarimovie-server-windows-x86_64.tar.gz',
        'pencarimovie-server-macos-aarch64.tar.gz',
        'pencarimovie-server_release_universal.apk')) {
    $u = 'https://github.com/aiskendi/pencarimovie-server/releases/latest/download/' + $a
    '{0,-46} : {1}' -f $a, (Get-Status $u)
}
