# Cheetah iOS host

The iOS app is a UIKit host for the same Rust actor and op protocol used on Android. It does not use SwiftUI or React Native.

![Cheetah running in the iOS Simulator](../docs/ios.png)

## Build on a Mac

```bash
cd ios
bash build-rust.sh

brew install xcodegen
xcodegen generate
open Cheetah.xcodeproj
```

Select a signing team in Xcode, then run on a simulator or device.

`build-rust.sh` compiles the Rust core for device and simulator architectures and creates `ios/Libs/Runtime.xcframework`. The Xcode target links that XCFramework.

## Gateway

Run the local gateway on the Mac:

```bash
cargo run -p gateway
```

The iOS Simulator shares the Mac's loopback, so `ws://127.0.0.1:9001` works as-is. For a physical device, change the gateway URL in `Cheetah/RustHost.swift` to the Mac's LAN address and add the appropriate ATS exception.

## What is implemented

- C ABI (`cheetah_new`, `cheetah_resize`, `cheetah_event`, `cheetah_free`) over the shared Rust core.
- Ops callback posts JSON to the main queue.
- CoreText text measurement callback on the actor thread.
- Native `UILabel`, custom `UIControl` buttons, `UITextField`, and `UIScrollView` views driven by absolute frames.
- Variable-height list slots, scroll callbacks, scroll-to-bottom, connection status, and offline outbox from the shared core.

## Verification

CI builds the Rust XCFramework, generates the Xcode project, builds both device and simulator targets, launches the app in an iPhone simulator, and publishes the screenshot above. The app has not yet been run on a physical device.

