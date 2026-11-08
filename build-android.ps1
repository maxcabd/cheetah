param(
    [switch]$Install
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "==> Building Rust runtime for Android (x86_64)"
cargo build -p runtime --target x86_64-linux-android --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Building Rust runtime for Android (arm64)"
cargo build -p runtime --target aarch64-linux-android --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Copying native libraries into the APK"
Copy-Item "target/x86_64-linux-android/release/libruntime.so" "android/app/src/main/jniLibs/x86_64/libruntime.so" -Force
Copy-Item "target/aarch64-linux-android/release/libruntime.so" "android/app/src/main/jniLibs/arm64-v8a/libruntime.so" -Force

Write-Host "==> Assembling debug APK"
Push-Location android
.\gradlew.bat :app:assembleDebug
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { exit $code }

$apk = Join-Path $root "android/app/build/outputs/apk/debug/app-debug.apk"
Write-Host "APK: $apk"

if ($Install) {
    Write-Host "==> Installing and launching"
    adb install -r $apk
    adb shell am force-stop com.example.rustui
    adb shell am start -n com.example.rustui/.MainActivity
}
