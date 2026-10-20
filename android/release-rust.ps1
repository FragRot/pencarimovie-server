<#
  One-shot release for the RUST android app (android/pencarimovie-server).

  Same loop as release.ps1 for the TDLib app — bump, build, stage with sha256,
  publish version.json + APKs, verify what the release actually serves, then commit
  the tracked repo — with the differences this app requires:

    * The engine is port/rust, cross-built for four ABIs by
      scripts\build-rust-apk.bat, which also stages android\_apk_dist. This script
      drives that bat instead of calling gradle directly, so the Rust binaries and
      the jniLibs staging can never be forgotten.
    * Its version comes from GRADLE PROPERTIES (versionNameOverride /
      versionCodeOverride), not from app/build.gradle, so the bump is written to
      android/pencarimovie-server/gradle.properties.
    * Two native libraries must be inside every APK: libpencarimovie_rust.so (the
      server) and libcloudflared.so (the bionic tunnel binary, needed because
      Android 10+ cannot exec a file from app data). jniLibs/ is gitignored, so a
      fresh clone builds a DEAD APK — this refuses to publish one.
    * It publishes to its OWN feed (apk-rust-latest on pencarimovie-downloader), so
      it can never clobber the TDLib app's apk-server-latest.

  Usage (run from android/, or pass -Path):
    .\release-rust.ps1                    # rebuild the current version and republish
    .\release-rust.ps1 -Bump              # bump the patch first (1.0.0 -> 1.0.1)
    .\release-rust.ps1 -Version 1.1.0     # force a version
    .\release-rust.ps1 -SkipBuild         # reuse the APKs already staged
    .\release-rust.ps1 -SkipUpload        # everything except `gh release upload`
    .\release-rust.ps1 -SkipRepoPush      # don't commit/push the tracked repo
#>
[CmdletBinding()]
param(
    [switch]$Bump,
    [string]$Version,
    [switch]$SkipBuild,
    [switch]$SkipUpload,
    [switch]$SkipRepoPush,
    [string]$NotesFile = 'release-notes-rust.md'
)

$ErrorActionPreference = 'Stop'

# The SAME repo, tag, `version.json` and asset names as `android/release.ps1`.
# This app REPLACES `com.pencarimovie.server` (same applicationId, same signing key,
# continuing versionCode), so publishing to a separate "rust" feed would leave every
# existing install on the old updater and split the fleet in two. One feed, one name.
$GhRepo = 'aiskendi/pencarimovie-server'
$Tag = 'apk-server-latest'
# The four ABI splits plus the universal APK. The universal one is the default
# download (`apkUrl` / `sha256`) because it installs anywhere; the per-ABI files stay
# for size-conscious updates.
$Abis = @('arm64-v8a', 'armeabi-v7a', 'x86', 'x86_64', 'universal')
$ApkPrefix = 'pencarimovie-server_release'

$root = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
# The NATIVE app lives in android/pencarimovie-rust. Pointing this at
# `pencarimovie-server` (the retired TDLib tree) bumped the wrong
# gradle.properties and published a version that did not match the built APK.
$appDir = Join-Path $root 'pencarimovie-rust'
$props = Join-Path $appDir 'gradle.properties'
$dist = Join-Path $root '_apk_dist'
$repo = (Resolve-Path (Join-Path $root '..')).Path
$buildBat = Join-Path $repo 'scripts\build-rust-apk.bat'
$baseUrl = "https://github.com/$GhRepo/releases/download/$Tag"

# --- 1. version -----------------------------------------------------------------
# The version lives in gradle.properties as versionNameOverride/versionCodeOverride.
# Guard the same way release.ps1 does: these files are rewritten every run, and a
# BOM-less read as ANSI + write as UTF-8 re-encodes every non-ASCII byte, compounding
# until Gradle dies ("UTF16 String size …"). ReadAllText with UTF8 + WriteAllText
# without BOM is the pair that keeps it stable.
foreach ($f in @($props, (Join-Path $appDir 'app\build.gradle'))) {
    $size = (Get-Item $f).Length
    if ($size -gt 512KB) {
        throw "$f is $size bytes - refusing to run. It has been corrupted by encoding round-trips (see AGENTS.md 'build.gradle encoding bloat')."
    }
}
$propsText = [System.IO.File]::ReadAllText($props, [System.Text.Encoding]::UTF8)
if (-not $Version) {
    $m = [regex]::Match($propsText, '(?m)^versionNameOverride\s*=\s*([0-9]+\.[0-9]+\.[0-9]+)\s*$')
    $Version = if ($m.Success) { $m.Groups[1].Value } else { '1.0.0' }
}
$p = $Version.Split('.') | ForEach-Object { [int]$_ }
if ($p.Count -ne 3) { throw "Version must be MAJOR.MINOR.PATCH (got '$Version')" }
if ($Bump) { $Version = "$($p[0]).$($p[1]).$($p[2] + 1)"; $p = $Version.Split('.') | ForEach-Object { [int]$_ } }
$versionCode = $p[0] * 10000 + $p[1] * 100 + $p[2]

function Set-Prop([string]$text, [string]$key, [string]$value) {
    $line = "$key=$value"
    if ([regex]::IsMatch($text, "(?m)^$key\s*=.*$")) {
        return [regex]::Replace($text, "(?m)^$key\s*=.*$", $line)
    }
    return ($text.TrimEnd() + "`n" + $line + "`n")
}
$newProps = Set-Prop (Set-Prop $propsText 'versionNameOverride' $Version) 'versionCodeOverride' "$versionCode"
if ($newProps -ne $propsText) {
    [System.IO.File]::WriteAllText($props, $newProps, [System.Text.UTF8Encoding]::new($false))
    Write-Host "version          : $Version ($versionCode)  <- written to gradle.properties"
}
else {
    Write-Host "version          : $Version ($versionCode)"
}

# Release-page copy (optional). {{VERSION}} / {{VERSIONCODE}} substituted.
$notesPath = if ([System.IO.Path]::IsPathRooted($NotesFile)) { $NotesFile } else { Join-Path $root $NotesFile }
$notesTmp = ''
if (Test-Path $notesPath) {
    $notesBody = [System.IO.File]::ReadAllText($notesPath, [System.Text.Encoding]::UTF8).Replace('{{VERSION}}', $Version).Replace('{{VERSIONCODE}}', "$versionCode")
    $notesTmp = Join-Path ([System.IO.Path]::GetTempPath()) 'pcm-rust-release-notes.md'
    [System.IO.File]::WriteAllText($notesTmp, $notesBody, [System.Text.UTF8Encoding]::new($false))
}
else {
    Write-Host "notes            : $NotesFile not found - release page left as is"
}

# --- 2. build -------------------------------------------------------------------
# Through scripts\build-apk.ps1: it runs the same build DETACHED and prints a
# progress line every 5 s. Calling the .bat inline made a multi-minute build look
# like a frozen terminal (the log only appeared at the end), and killing the
# shell killed the build mid-way.
if (-not $SkipBuild) {
    if (-not (Test-Path $buildBat)) { throw "missing $buildBat" }
    Write-Host 'building         : scripts\build-apk.ps1 -> build-rust-apk.bat (4 ABIs + gradle) ...'
    $runner = Join-Path $repo 'scripts\build-apk.ps1'
    $buildArgs = @()
    if ($BuildDebug) { $buildArgs += '--debug' }
    & powershell -NoProfile -ExecutionPolicy Bypass -File $runner -Args $buildArgs
    if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE) - see _apk_build.log" }
}

# --- 3. stage -------------------------------------------------------------------
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$outDir = Join-Path $appDir 'app\build\outputs\apk\release'
$sha = @{}
foreach ($abi in $Abis) {
    $dest = Join-Path $dist "$($ApkPrefix)_$abi.apk"
    # The gradle output is authoritative: refresh the staged copy whenever it is NEWER.
    # Copying only when the destination is MISSING (the old behaviour) meant a rebuild
    # that changed the APK was silently published as the previous build — observed when
    # applicationId changed and the staged universal APK kept its old hash. -SkipBuild
    # still works: with no newer gradle output the already-staged file is kept.
    $src = Get-ChildItem $outDir -Filter "*_$abi.apk" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($src) {
        $destTime = if (Test-Path $dest) { (Get-Item $dest).LastWriteTime } else { [datetime]::MinValue }
        if ($src.LastWriteTime -gt $destTime) { Copy-Item -Force $src.FullName $dest }
    }
    if (-not (Test-Path $dest)) { throw "no APK for '$abi' in $dist or $outDir (run without -SkipBuild)" }
    $h = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLower()
    [System.IO.File]::WriteAllText("$dest.sha256", $h)          # no newline, no BOM
    $sha[$abi] = $h
    Write-Host ("staged           : {0,-12} {1,7:N1} MB  {2}" -f $abi, ((Get-Item $dest).Length / 1MB), $h)
}

# An APK without libpencarimovie_rust.so installs, starts, then dies with
# UnsatisfiedLinkError; without libcloudflared.so the tunnel can never be enabled
# (Android 10+ cannot exec from app data). jniLibs/ is gitignored, so
# :app:assembleRelease SUCCEEDS on a fresh clone — refuse to publish that.
Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction SilentlyContinue
foreach ($abi in $Abis) {
    $apk = Join-Path $dist "$($ApkPrefix)_$abi.apk"
    $zip = [System.IO.Compression.ZipFile]::OpenRead($apk)
    try {
        $names = @($zip.Entries | ForEach-Object { $_.FullName })
        $hasServer = @($names | Where-Object { $_ -match '^lib/.*/libpencarimovie_rust\.so$' }).Count -gt 0
        $hasCf = @($names | Where-Object { $_ -match '^lib/.*/libcloudflared\.so$' }).Count -gt 0
    }
    finally { $zip.Dispose() }
    if (-not $hasServer) { throw "$apk contains no libpencarimovie_rust.so - jniLibs/ is missing (gitignored). Refusing to publish an APK that crashes on start." }
    if (-not $hasCf) { throw "$apk contains no libcloudflared.so - the Cloudflare tunnel would fail with 'Permission denied'. Rebuild with scripts\build-rust-apk.bat." }
    Write-Host ("native libs      : {0,-12} server + cloudflared present" -f $abi)
}

$json = [ordered]@{
    # Same applicationId as the TDLib app: this build REPLACES com.pencarimovie.server
    # in place (identical signing key too), so the feed's applicationId must match or
    # the app's own updater would reject it.
    applicationId = 'com.pencarimovie.server'
    versionName   = $Version
    versionCode   = $versionCode
    apks          = [ordered]@{}
    apkUrl        = "$baseUrl/$($ApkPrefix)_universal.apk"
    sha256        = $sha['universal']
    builtAt       = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    commit        = (& git -C $repo rev-parse --short HEAD)
}
foreach ($abi in $Abis) {
    $json.apks[$abi] = [ordered]@{ url = "$baseUrl/$($ApkPrefix)_$abi.apk"; sha256 = $sha[$abi] }
}
$versionPath = Join-Path $dist 'version.json'
[System.IO.File]::WriteAllText($versionPath, ($json | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "staged           : version.json (BOM-free)"

# --- 4. upload ------------------------------------------------------------------
if (-not $SkipUpload) {
    $files = @()
    foreach ($abi in $Abis) {
        $files += (Join-Path $dist "$($ApkPrefix)_$abi.apk")
        $files += (Join-Path $dist "$($ApkPrefix)_$abi.apk.sha256")
    }
    $files += $versionPath

    $releaseTitle = "PencariMovie Server - rolling build feed ($Version)"
    # (unchanged wording on purpose: the release page is the product's page, and it
    # must not mention the engine — the app has no "rust" name any more.)
    # `gh release view` writes "release not found" to STDERR when the tag does not exist yet,
    # and with $ErrorActionPreference='Stop' a native command's stderr becomes a terminating
    # NativeCommandError — it aborted the whole release BEFORE the create (observed: staged all
    # five APKs, then died on a missing release it was about to create). Same trap as the git
    # step below: relax the preference, decide on the exit code.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & gh release view $Tag --repo $GhRepo 2>&1 | Out-Null
    $releaseExists = ($LASTEXITCODE -eq 0)
    if (-not $releaseExists) {
        Write-Host "release          : creating $Tag on $GhRepo"
        if ($notesTmp) { & gh release create $Tag --repo $GhRepo --latest=false --title $releaseTitle --notes-file $notesTmp }
        else { & gh release create $Tag --repo $GhRepo --latest=false --title $releaseTitle --notes 'Rolling build feed. See version.json for the current version.' }
        $createExit = $LASTEXITCODE
        $ErrorActionPreference = $prevEap
        if ($createExit -ne 0) { throw "gh release create $Tag failed" }
    }
    else {
        $ErrorActionPreference = $prevEap
    }

    Write-Host "uploading        : $($files.Count) assets -> $Tag"
    & gh release upload $Tag $files --clobber --repo $GhRepo
    if ($LASTEXITCODE -ne 0) { throw 'gh release upload failed' }

    if ($notesTmp) {
        & gh release edit $Tag --repo $GhRepo --title $releaseTitle --notes-file $notesTmp
        if ($LASTEXITCODE -ne 0) { throw 'gh release edit $Tag failed' }
        Write-Host "notes            : release page re-titled and re-noted ($Version)"
    }

    # --- 5. verify (asset size + sha256 sidecars round-trip) ---
    $ErrorActionPreference = 'Continue'
    $api = (& gh api "repos/$GhRepo/releases/tags/$Tag" 2>$null) | ConvertFrom-Json
    $ErrorActionPreference = $prevEap
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('pmrust-verify-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $ErrorActionPreference = 'Continue'
        & gh release download $Tag --repo $GhRepo --pattern '*.sha256' --dir $tmp --clobber 2>&1 | Out-Null
        $dlExit = $LASTEXITCODE
        $ErrorActionPreference = $prevEap
        if ($dlExit -ne 0) { throw 'could not download the uploaded .sha256 sidecars' }
        foreach ($abi in $Abis) {
            $name = "$($ApkPrefix)_$abi.apk"
            $asset = $api.assets | Where-Object { $_.name -eq $name }
            if (-not $asset) { throw "asset missing on the release: $name" }
            $localBytes = (Get-Item (Join-Path $dist $name)).Length
            if ($asset.size -ne $localBytes) { throw "size mismatch for ${name}: local=$localBytes remote=$($asset.size)" }
            $remoteSha = (Get-Content (Join-Path $tmp "$name.sha256") -Raw).Trim()
            if ($remoteSha -ne $sha[$abi]) { throw "sha256 mismatch for ${name}: local=$($sha[$abi]) remote=$remoteSha" }
            Write-Host ("verified         : {0,-12} {1} bytes  sha256 OK" -f $abi, $asset.size)
        }
    }
    finally { Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue }
}
else {
    Write-Host 'upload           : skipped'
}

# --- 6. tracked repo ------------------------------------------------------------
# Same two rails as release.ps1, both learned from an aborted release:
#   1. git writes LF/CRLF warnings to STDERR; with $ErrorActionPreference='Stop' a
#      native command's stderr becomes a terminating NativeCommandError, which once
#      aborted the script AFTER the upload. stderr is suppressed; exit codes decide.
#   2. `git add -u` stages EVERY tracked modification, and this workspace keeps
#      composer.json/lock and patches/ dirty for long stretches. Those are unstaged.
if (-not $SkipRepoPush) {
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & git -C $repo -c core.safecrlf=false add -u 2>&1 | Out-Null

        $staged = @(& git -C $repo diff --cached --name-only 2>&1)
        $unrelated = @($staged | Where-Object { $_ -match '^(composer\.(json|lock)|patches/)' })
        if ($unrelated.Count -gt 0) {
            foreach ($p in $unrelated) { & git -C $repo restore --staged -- $p 2>&1 | Out-Null }
            Write-Host ("repo             : left {0} unrelated path(s) unstaged" -f $unrelated.Count)
        }

        & git -C $repo diff --cached --quiet 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Host 'repo             : no tracked changes to commit'
        }
        else {
            & git -C $repo -c core.safecrlf=false commit -m "chore(release): rust $Version" 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'git commit failed' }
            & git -C $repo push origin HEAD 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'git push failed' }
            Write-Host 'repo             : committed + pushed (tracked files only)'
        }
    }
    finally { $ErrorActionPreference = $prevEap }
}
else {
    Write-Host 'repo             : push skipped'
}

Write-Host ''
Write-Host "done: $Version ($versionCode)"
foreach ($abi in $Abis) { Write-Host "  $baseUrl/$($ApkPrefix)_$abi.apk" }
Write-Host "  $baseUrl/version.json"
