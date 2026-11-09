# End-to-end profiling

The Rust benchmark in `tools/bench` measures layout, diffing, and serialization only. These tools measure the whole host path: Rust actor work, native text measurement, view application, and the platform renderer.

## iOS

The `testScrollPerformance` UI test uses `XCTOSSignpostMetric.scrollingAndDecelerationMetric` around a fast scroll of the message list. CI publishes the matching portion of the test log to:

- `ios-preview` branch: `ci-preview/ios-performance.txt` (iOS 26)
- `ios-18-preview` branch: `ci-preview/ios-18-performance.txt` (iOS 18.5 blur fallback)

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

## Caveats

Simulator and emulator results are useful for relative regressions and pipeline changes, not absolute device performance. Native text measurement and host rendering are included here; the Rust benchmark excludes them. Device testing remains the final performance gate.
