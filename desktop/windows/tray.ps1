# PencariMovie Server - Windows system-tray controller.
#
#   start-tray.bat            runs this hidden and starts the server
#   powershell -File tray.ps1 -StartServer -Port 8088
#
# Right-click the icon: open the dashboard / add-on page, copy the LAN URL,
# restart, stop, toggle "run at login", open the log, exit. Double-click opens
# the dashboard.
#
# No third-party module is used (System.Windows.Forms only), so this works on a
# stock Windows install. The server itself is a separate process
# (`pencarimovie-server.exe`), so the tray can be closed without stopping it —
# that is what "Stop server" is for.

param(
    [switch]$StartServer,
    [int]$Port = 0,
    # Accepted for compatibility with the `pms` CLI (pencarimovie-windows.bat
    # runs `tray.ps1 -Port … -OpenUrl … -StopBat … -StartServer`). PowerShell -File
    # REJECTS an unknown parameter, so these must exist even when unused.
    [string]$OpenUrl = '',
    [string]$StopBat = ''
)

$ErrorActionPreference = 'SilentlyContinue'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $here

$exe = Join-Path $here 'pencarimovie-server.exe'
$storage = Join-Path $here 'storage'
$pidFile = Join-Path $storage 'server.pid'
$logFile = Join-Path $storage 'server.log'
$icoFile = Join-Path $here 'pencarimovie.ico'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runName = 'PencariMovie Server'

New-Item -ItemType Directory -Force -Path $storage | Out-Null
if (-not $Port) {
    if ($env:PORT) { $Port = [int]$env:PORT } else { $Port = 8088 }
}
$base = "http://127.0.0.1:$Port"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

function Get-LanIp {
    # Same rule the launchers use: the interface owning the default route.
    $line = (& route.exe print -4 0.0.0.0 2>$null | Select-String -Pattern '^\s+0\.0\.0\.0\s+0\.0\.0\.0' | Select-Object -First 1)
    if ($line) {
        $parts = ($line.ToString() -split '\s+') | Where-Object { $_ }
        if ($parts.Count -ge 4) { return $parts[3] }
    }
    return '127.0.0.1'
}

function Test-Server {
    try {
        $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 "$base/api/version"
        return ($r.StatusCode -eq 200)
    }
    catch { return $false }
}

function Get-ServerProcess {
    $p = Get-Content $pidFile -ErrorAction SilentlyContinue
    if ($p) {
        $proc = Get-Process -Id ([int]$p) -ErrorAction SilentlyContinue
        if ($proc -and $proc.ProcessName -like 'pencarimovie-server*') { return $proc }
    }
    # Fall back to a name scan (the pid file can be stale after a reboot).
    return (Get-Process -Name 'pencarimovie-server' -ErrorAction SilentlyContinue | Select-Object -First 1)
}

function Start-Server {
    if (Test-Server) { return }
    $proc = Get-ServerProcess
    if ($proc) { return }
    if (-not (Test-Path $exe)) {
        [void][System.Windows.Forms.MessageBox]::Show(
            "pencarimovie-server.exe was not found next to tray.ps1.`nReinstall the package (README.txt).",
            'PencariMovie Server', 'OK', 'Error')
        return
    }
    $p = Start-Process -FilePath $exe -ArgumentList @('--port', "$Port") `
        -WorkingDirectory $here -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput $logFile -RedirectStandardError "$logFile.err"
    if ($p) { Set-Content -Path $pidFile -Value $p.Id -Encoding ASCII }
}

function Stop-Server {
    $proc = Get-ServerProcess
    if ($proc) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 300
    }
    # Anything still bound to the port but not in the pid file.
    Get-Process -Name 'pencarimovie-server' -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -ne $PID } |
    Stop-Process -Force -ErrorAction SilentlyContinue
    Remove-Item $pidFile -ErrorAction SilentlyContinue
}

function Restart-Server {
    Stop-Server
    Start-Sleep -Milliseconds 400
    Start-Server
    Update-Tip
}

function Open-Page([string]$path) {
    Start-Server
    Start-Sleep -Milliseconds 250
    Start-Process "$base$path"
}

function Copy-AddonUrl {
    Start-Server
    $lan = Get-LanIp
    $url = "http://${lan}:$Port/manifest.json"
    [System.Windows.Forms.Clipboard]::SetText($url)
    Show-Balloon "Add-on URL copied: $url"
}

function Show-Balloon([string]$text) {
    $script:notify.BalloonTipTitle = 'PencariMovie Server'
    $script:notify.BalloonTipText = $text
    $script:notify.ShowBalloonTip(3000)
}

function Test-Autostart {
    $v = (Get-ItemProperty -Path $runKey -Name $runName -ErrorAction SilentlyContinue).$runName
    return [bool]$v
}

function Set-Autostart([bool]$on) {
    if ($on) {
        $cmd = "powershell -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$here\tray.ps1`" -StartServer -Port $Port"
        New-Item -Path $runKey -Force | Out-Null
        Set-ItemProperty -Path $runKey -Name $runName -Value $cmd
        Show-Balloon 'The server will start at login.'
    }
    else {
        Remove-ItemProperty -Path $runKey -Name $runName -ErrorAction SilentlyContinue
        Show-Balloon 'Login start disabled.'
    }
    $script:autoItem.Checked = Test-Autostart
}

function Update-Tip {
    $state = if (Test-Server) { 'running' } else { 'stopped' }
    $script:notify.Text = "PencariMovie Server - $state on port $Port"
}

# ── tray icon + menu ────────────────────────────────────────────────────────
$script:notify = New-Object System.Windows.Forms.NotifyIcon
if (Test-Path $icoFile) {
    $script:notify.Icon = New-Object System.Drawing.Icon($icoFile)
}
else {
    $script:notify.Icon = [System.Drawing.SystemIcons]::Application
}
$script:notify.Visible = $true

$menu = New-Object System.Windows.Forms.ContextMenuStrip
$openItem = $menu.Items.Add('Open dashboard')
$openItem.add_Click({ Open-Page '/' })
$addonItem = $menu.Items.Add('Open add-on page')
$addonItem.add_Click({ Open-Page '/#addon' })
$copyItem = $menu.Items.Add('Copy add-on URL')
$copyItem.add_Click({ Copy-AddonUrl })
[void]$menu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
$restartItem = $menu.Items.Add('Restart server')
$restartItem.add_Click({ Restart-Server })
$stopItem = $menu.Items.Add('Stop server')
$stopItem.add_Click({ Stop-Server; Update-Tip })
$script:autoItem = $menu.Items.Add('Run at login')
$script:autoItem.CheckOnClick = $true
$script:autoItem.Checked = Test-Autostart
$script:autoItem.add_Click({ Set-Autostart $script:autoItem.Checked })
$logItem = $menu.Items.Add('Open log')
$logItem.add_Click({ if (Test-Path $logFile) { Start-Process notepad.exe $logFile } })
$updateItem = $menu.Items.Add('Check for updates...')
$updateItem.add_Click({
        try {
            $r = Invoke-RestMethod -Uri "$base/api/update/check" -TimeoutSec 25
            if ($r.ok -and $r.update_available) {
                $ans = [System.Windows.Forms.MessageBox]::Show(
                    ("Update available: {0}`nInstalled: {1} (build {2})`n`nDownload and restart now?" -f $r.latest, $r.current, $r.build),
                    'PencariMovie Server', 'YesNo', 'Question')
                if ($ans -eq [System.Windows.Forms.DialogResult]::Yes) {
                    Show-Balloon 'Downloading the update...'
                    $a = Invoke-RestMethod -Uri "$base/api/update/apply" -Method POST -Body '{}' -ContentType 'application/json' -TimeoutSec 900
                    if ($a.ok) { Show-Balloon $a.message } else {
                        [void][System.Windows.Forms.MessageBox]::Show($a.error, 'PencariMovie Server', 'OK', 'Error')
                    }
                }
            }
            else {
                $msg = if ($r.ok) { "Already up to date: $($r.current) (build $($r.build))" } else { $r.error }
                [void][System.Windows.Forms.MessageBox]::Show($msg, 'PencariMovie Server', 'OK', 'Information')
            }
        }
        catch {
            [void][System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'PencariMovie Server', 'OK', 'Error')
        }
    })
[void]$menu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
$exitItem = $menu.Items.Add('Exit tray (leaves server running)')
$exitItem.add_Click({ $script:notify.Visible = $false; [System.Windows.Forms.Application]::Exit() })

$script:notify.ContextMenuStrip = $menu
$script:notify.add_MouseDoubleClick({ Open-Page '/' })
$script:notify.add_MouseClick({
        param($sender, $e)
        if ($e.Button -eq [System.Windows.Forms.MouseButtons]::Left) { Open-Page '/' }
    })

if ($StartServer) { Start-Server }
Update-Tip
# NB: PowerShell 5.1 has no `if` *expression* - assign the branch result first.
$stateText = if (Test-Server) { "running on http://127.0.0.1:$Port" } else { 'starting...' }
Show-Balloon "PencariMovie Server: $stateText"

[System.Windows.Forms.Application]::Run()
$script:notify.Visible = $false
