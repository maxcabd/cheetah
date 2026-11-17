param(
    # App crate to serve with hot patching (apps/<name>).
    [string]$App = "nightshade",
    # Extra arguments for the app's dev server, e.g. -ServerArgs "--demo"
    [string]$ServerArgs = ""
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

Write-Host "==> dx serve --hot-patch -p $App (save a file in apps/$App to patch)"
$dxArgs = @("serve", "--hot-patch", "-p", $App, "--bin", $App)
if ($ServerArgs) {
    # Joined with `=` so dx doesn't parse a leading `--` as its own flag.
    $dxArgs += "--args=$ServerArgs"
}
& dx @dxArgs
