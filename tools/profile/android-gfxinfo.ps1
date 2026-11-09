param(
    [string]$Package = "com.example.rustui",
    [int]$Iterations = 6
)

$ErrorActionPreference = "Stop"

function Invoke-Adb {
    param([string[]]$Command)
    & adb @Command
    if ($LASTEXITCODE -ne 0) {
        throw "adb $($Command -join ' ') failed"
    }
}

Write-Host "==> Launching $Package"
Invoke-Adb @("shell", "am", "force-stop", $Package) | Out-Null
Invoke-Adb @("shell", "am", "start", "-n", "$Package/.MainActivity") | Out-Null
Start-Sleep -Seconds 3
Invoke-Adb @("shell", "dumpsys", "gfxinfo", $Package, "reset") | Out-Null

Write-Host "==> Scrolling for $Iterations frames of measurement"
for ($i = 0; $i -lt $Iterations; $i++) {
    Invoke-Adb @("shell", "input", "swipe", "540", "1700", "540", "600", "250") | Out-Null
    Start-Sleep -Milliseconds 700
    Invoke-Adb @("shell", "input", "swipe", "540", "600", "540", "1700", "250") | Out-Null
    Start-Sleep -Milliseconds 700
}

$report = Invoke-Adb @("shell", "dumpsys", "gfxinfo", $Package)
$interesting = $report | Select-String -Pattern @(
    "Total frames rendered",
    "Janky frames",
    "50th percentile",
    "90th percentile",
    "95th percentile",
    "99th percentile",
    "Number Missed Vsync",
    "Number Slow UI thread",
    "Number Slow bitmap uploads",
    "Number Slow issue draw commands"
)

Write-Host ""
Write-Host "Android gfxinfo (end-to-end UI/render pipeline)"
$interesting | ForEach-Object { $_.Line.Trim() }
