# End-to-end proof on this machine: stream list -> payload -> real Range request.
#
#   powershell -NoProfile -File port\rust\tools\e2e_range.ps1 -Item 'pm:post:9000031194'
#
# Prints the stream count, the decoded payload, the HTTP status and the first
# bytes of the body. A working 206 starts with the container magic
# (`1a 45 df a3` = EBML/Matroska, `00 00 00 .. 66 74 79 70` = MP4 `ftyp`).

param(
    [string]$Base = 'http://127.0.0.1:8088',
    [string]$Item = 'pm:post:9000031194',
    [string]$Type = 'movie',
    [int]$Bytes = 262144
)

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$enc = [uri]::EscapeDataString($Item)
$listUrl = "$Base/stream/$Type/$enc.json"
Write-Host "GET $listUrl"
$list = Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 $listUrl
$streams = ($list.Content | ConvertFrom-Json).streams
Write-Host ("streams = {0}" -f @($streams).Count)
if (@($streams).Count -eq 0) {
    Write-Host $list.Content
    exit 1
}

$url = $streams[0].url
Write-Host "first    = $url"
$seg = ($url -split '/api/download/')[1]
$b64 = ($seg -split '/')[0]
$pad = $b64.Length % 4
if ($pad -ne 0) { $b64 = $b64 + ('=' * (4 - $pad)) }
$b64 = $b64.Replace('-', '+').Replace('_', '/')
$json = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($b64))
Write-Host "payload  = $json"
$payload = $json | ConvertFrom-Json
$code = $payload.short_code
if (-not $code) { Write-Host 'no short_code in payload'; exit 1 }

# Use the URL the stream item actually handed out (token prefix + payload path),
# which is the request Stremio makes. `?short_code=` is the fallback shape.
$dl = $url
Write-Host "range    = $dl  (bytes=0-$($Bytes - 1))"
$bodyPath = Join-Path (Get-Location).Path 'body.bin'
$hdrPath = Join-Path (Get-Location).Path 'hdr.txt'
curl.exe -s -o $bodyPath -D $hdrPath -r "0-$($Bytes - 1)" $dl
Get-Content $hdrPath

$len = (Get-Item $bodyPath).Length
Write-Host "bytes    = $len"
if ($len -gt 0) {
    $head = ([IO.File]::ReadAllBytes($bodyPath))[0..7]
    Write-Host ('magic    = ' + (($head | ForEach-Object { $_.ToString('x2') }) -join ' '))
}
