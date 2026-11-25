param(
    # App crate to serve with hot patching (apps/<name>).
    [string]$App = "nightshade",
    # Extra arguments for the app's dev server, e.g. -ServerArgs "--demo"
    [string]$ServerArgs = "",
    # Reach the phone through Cloudflare quick tunnels instead of the LAN,
    # for networks that keep devices apart (public Wi-Fi). Needs cloudflared.
    [switch]$Tunnel,
    # With -Tunnel: the local backend to tunnel too, baked in as
    # NIGHTSHADE_API_URL. 0 leaves NIGHTSHADE_API_URL alone.
    [int]$ApiPort = 8000
)

# Hot-patching dev server: edits to the app crate are patched into the
# running app (state kept) and connected devices update immediately.
# Pair with: .\build-android.ps1 -Install -DevServer
#
# dx relinks the app itself to enable patching, outside rustc, so it needs
# the Visual Studio build environment (LIB etc.) that a Developer shell has.
# This script loads it, so a normal PowerShell works.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

if (-not (Get-Command dx -ErrorAction SilentlyContinue)) {
    Write-Error "dx not found. Install it with: cargo install dioxus-cli --version 0.7.10 --locked"
}

if (-not $env:VCINSTALLDIR) {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) {
        Write-Error "Visual Studio Build Tools not found (needed for the MSVC libraries)."
    }
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    $vcvars = Join-Path $vs "VC\Auxiliary\Build\vcvars64.bat"
    if (-not (Test-Path $vcvars)) {
        Write-Error "vcvars64.bat not found under $vs"
    }
    Write-Host "==> Loading the Visual Studio build environment"
    # No stdin and quiet stderr: vcvars can otherwise stall in a hidden shell.
    cmd /c "`"$vcvars`" >nul 2>&1 <nul && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') {
            Set-Item -Path "env:$($Matches[1])" -Value $Matches[2]
        }
    }
}

# Start a quick tunnel to a local port; returns its process and public URL.
function Start-QuickTunnel([int]$Port) {
    $log = Join-Path $env:TEMP "cheetah-tunnel-$Port.log"
    Remove-Item $log -ErrorAction SilentlyContinue
    # cloudflared logs, including the URL, to stderr.
    $process = Start-Process cloudflared -PassThru -WindowStyle Hidden `
        -ArgumentList "tunnel", "--no-autoupdate", "--url", "http://localhost:$Port" `
        -RedirectStandardError $log -RedirectStandardOutput "$log.out"
    for ($i = 0; $i -lt 60; $i++) {
        Start-Sleep -Milliseconds 500
        $match = Select-String -Path $log -Pattern "https://[a-z0-9-]+\.trycloudflare\.com" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if ($match) {
            return @{ Process = $process; Url = $match.Matches[0].Value }
        }
    }
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    Write-Error "cloudflared gave no tunnel URL for port $Port (log: $log)"
}

$tunnels = @()
if ($Tunnel) {
    # Cloudflare deletes a quick tunnel while the PC sleeps, and its address
    # with it: stay awake, screen included, until this script ends. (Closing
    # the lid still sleeps.)
    $power = Add-Type -PassThru -Namespace Cheetah -Name Power -MemberDefinition `
        '[DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint flags);'
    # ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
    [void]$power::SetThreadExecutionState([uint32]2147483651)
    if (-not (Get-Command cloudflared -ErrorAction SilentlyContinue)) {
        Write-Error "cloudflared not found. Install it with: scoop install cloudflared"
    }
    Write-Host "==> Starting Cloudflare tunnels"
    $dev = Start-QuickTunnel 8787
    $tunnels += $dev
    if ($ApiPort -gt 0) {
        $api = Start-QuickTunnel $ApiPort
        $tunnels += $api
        # Read at build time (option_env!), so set before dx builds.
        $env:NIGHTSHADE_API_URL = $api.Url
        Write-Host "    backend:  $($api.Url) -> localhost:$ApiPort (start the backend yourself)"
    }
    $wss = $dev.Url -replace "^https://", "wss://"
    Write-Host "    dev:      $($dev.Url) -> localhost:8787"
    Write-Host ""
    $link = "cheetah://dev?url=$wss"
    & cargo run -q -p cheetah-cli --bin cheetah -- qr $link
    Write-Host "    Scan with the phone's camera, or open in Safari: $link"
    Write-Host ""
}

Write-Host "==> dx serve --hot-patch -p $App (save a file in apps/$App to patch)"
$dxArgs = @("serve", "--hot-patch", "-p", $App, "--bin", $App)
if ($ServerArgs) {
    # Joined with `=` so dx doesn't parse a leading `--` as its own flag.
    $dxArgs += "--args=$ServerArgs"
}
try {
    & dx @dxArgs
} finally {
    foreach ($t in $tunnels) {
        Stop-Process -Id $t.Process.Id -Force -ErrorAction SilentlyContinue
    }
}
