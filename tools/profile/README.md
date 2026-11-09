# End-to-end profiling

The Rust benchmark in `tools/bench` measures layout, diffing, and serialization only. These tools measure the whole host path: Rust actor work, native text measurement, view application, and the platform renderer.

## iOS

The `testScrollPerformance` UI test uses `XCTOSSignpostMetric.scrollingAndDecelerationMetric` around a fast scroll of the message list. CI publishes the matching portion of the test log to:

- `ios-preview` branch: `ci-preview/ios-performance.txt` and `ios-performance.json` (iOS 26)
- `ios-18-preview` branch: `ci-preview/ios-18-performance.txt` and `ios-18-performance.json` (iOS 18.5 blur fallback)

The JSON report comes from `xcresulttool get test-results metrics` and contains the full XCTest scroll metric, including any hitch/frame metrics the runtime provides.

Run locally on a Mac:

```bash
cd ios
xcodegen generate
xcodebuild test \
  -project Cheetah.xcodeproj \
  -scheme Cheetah \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  CODE_SIGNING_ALLOWED=NO
```

## Android

`android-gfxinfo.ps1` launches the app, resets `gfxinfo`, performs repeated swipes, and reports Android's frame/jank counters:

```powershell
.\tools\profile\android-gfxinfo.ps1
```

Attach an emulator or device first. The output includes total frames, janky frames, percentile frame times, missed vsyncs, and slow UI/render counters.

Sample on the API 35 emulator (`Pixel_4_API_35`, 1080x2280):

```text
Total frames rendered: 581
Janky frames: 25 (4.30%)
50th percentile: 20ms
90th percentile: 26ms
95th percentile: 29ms
99th percentile: 42ms
Number Missed Vsync: 9
Number Slow UI thread: 7
Number Slow issue draw commands: 20
```

## Current iOS scroll samples

XCTest measured the same fast drag/deceleration on the two CI paths:

- iOS 26 / Liquid Glass: 0.837 s average
- iOS 18.5 / blur fallback: 0.844 s average

The duration is only the first XCTest metric. The published JSON is the source for frame/hitch metrics.

## Caveats

Simulator and emulator results are useful for relative regressions and pipeline changes, not absolute device performance. Native text measurement and host rendering are included here; the Rust benchmark excludes them. Device testing remains the final performance gate.
