# Builds the shared-hosting (cPanel / Apache) deployment ZIP.
#
#   powershell -NoProfile -File scripts\package-shared-hosting.ps1
#
# Ships exactly what the app needs without a process-capable host: no bin/
# (the 54 MB FrankenPHP runtime is useless on shared hosting), no patches/,
# scripts/, android/ or node_modules. Upload the ZIP with cPanel File Manager
# and use Extract — do NOT upload it over FTP, several free hosts silently
# drop large transfers.
param(
    [string]$Out = "pencarimovie-shared-hosting.zip"
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stage = Join-Path $env:TEMP ("pm_shared_" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage -Force | Out-Null

Write-Host "Staging in $stage ..."
foreach ($f in 'index.php', 'backend.php', 'composer.json', '.htaccess', 'install.php', 'SHARED-HOSTING.md') {
    $src = Join-Path $root $f
    if (Test-Path $src) { Copy-Item $src $stage -Force }
}
foreach ($d in 'public', 'vendor') {
    Copy-Item (Join-Path $root $d) $stage -Recurse -Force
}

# storage/ must exist and be writable by PHP; the cPanel account owns it.
foreach ($d in 'storage', 'storage\cache', 'storage\sessions', 'storage\reports', 'storage\bin') {
    New-Item -ItemType Directory -Path (Join-Path $stage $d) -Force | Out-Null
}
[IO.File]::WriteAllText((Join-Path $stage 'storage\.gitkeep'), '', (New-Object System.Text.UTF8Encoding($false)))

# Without this stamp FD_APP_VERSION falls back to the built-in default and the
# WordPress version gate can answer "Update required" on every /stream request.
$tagFile = Join-Path $root '.release-tag'
$tag = if (Test-Path $tagFile) { (Get-Content $tagFile -Raw).Trim() } else { 'v2.7.9' }
[IO.File]::WriteAllText((Join-Path $stage '.release-tag'), $tag, (New-Object System.Text.UTF8Encoding($false)))

$outPath = if ([IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path $root $Out }
if (Test-Path $outPath) { Remove-Item $outPath -Force }
& tar.exe -a -c -f $outPath -C $stage .
if ($LASTEXITCODE -ne 0) { throw "tar failed with exit code $LASTEXITCODE" }

$zip = Get-Item $outPath
$files = (Get-ChildItem $stage -Recurse -File).Count
Remove-Item $stage -Recurse -Force

Write-Host ("Built {0} ({1:N1} MB, {2} files) at version {3}" -f $zip.Name, ($zip.Length / 1MB), $files, $tag)
Write-Host "Next: cPanel -> File Manager -> public_html -> Upload this ZIP -> Extract."
