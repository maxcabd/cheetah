param(
    # Install on the connected device/emulator and launch -App.
    [switch]$Install,
    # App compiled into the runtime to launch: nightshade, nightshade-demo, chat.
    [string]$App = "nightshade",
    # Nightshade backend URL baked into the build (NIGHTSHADE_API_URL).
    # A localhost URL is forwarded to this machine with `adb reverse`.
    [string]$ApiUrl,
    # Build every ABI even when installing (default: only the device's ABI).
    [switch]$AllAbis
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

$abis = @{
    "arm64-v8a" = "aarch64-linux-android"
    "x86_64"    = "x86_64-linux-android"
}

$targets = @("x86_64", "arm64-v8a")
if ($Install -and -not $AllAbis) {
    # Through cmd: PowerShell 5.1 turns adb's stderr into a terminating error.
    $deviceAbi = (cmd /c "adb shell getprop ro.product.cpu.abi 2>nul" | Out-String).Trim()
    if (-not $deviceAbi) {
        Write-Error "No device found. Connect a phone with USB debugging (adb devices) or start an emulator."
    }
    if (-not $abis.ContainsKey($deviceAbi)) {
        Write-Error "Unsupported device ABI '$deviceAbi'. Supported: $($abis.Keys -join ', ')"
    }
    $targets = @($deviceAbi)
    Write-Host "==> Device ABI: $deviceAbi (building only this; pass -AllAbis for both)"
}

if ($ApiUrl) {
    $env:NIGHTSHADE_API_URL = $ApiUrl
    Write-Host "==> NIGHTSHADE_API_URL = $ApiUrl"
}

foreach ($abi in $targets) {
    $triple = $abis[$abi]
    Write-Host "==> Building Rust runtime for Android ($abi)"
    cmd /c "cargo build -p runtime --target $triple --release --features source-map 2>&1"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    New-Item -ItemType Directory -Force "android/app/src/main/jniLibs/$abi" | Out-Null
    Copy-Item "target/$triple/release/libruntime.so" "android/app/src/main/jniLibs/$abi/libruntime.so" -Force
}

Write-Host "==> Assembling debug APK"
Push-Location android
.\gradlew.bat :app:assembleDebug
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { exit $code }

$apk = Join-Path $root "android/app/build/outputs/apk/debug/app-debug.apk"
Write-Host "APK: $apk"

if ($Install) {
    if ($ApiUrl -and $ApiUrl -match '^https?://(localhost|127\.0\.0\.1):(\d+)') {
        $port = $Matches[2]
        Write-Host "==> Forwarding device localhost:$port to this machine (adb reverse)"
        adb reverse "tcp:$port" "tcp:$port" | Out-Null
    }

    Write-Host "==> Installing and launching $App"
    adb install -r $apk
    adb shell am force-stop com.example.rustui
    adb shell am start -n com.example.rustui/.MainActivity --es app $App
}
