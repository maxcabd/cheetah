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

Build with Xcode 26 to get Liquid Glass. Select a signing team in Xcode, then run on a simulator or device.

`build-rust.sh` compiles the Rust core for device and simulator architectures and creates `ios/Libs/Runtime.xcframework`. The Xcode target links that XCFramework.

## Gateway

Run the local gateway on the Mac:

```bash
cargo run -p gateway
```

The iOS Simulator shares the Mac's loopback, so `ws://127.0.0.1:9001` works as-is. For a physical device, change the gateway URL in `Cheetah/RustHost.swift` to the Mac's LAN address and add the appropriate ATS exception.

## Liquid Glass

On iOS 26, the header and composer are rendered as Liquid Glass containers using `UIGlassEffect`. The list extends underneath both bars and scrolls behind the material. Clear and Send are interactive glass controls.

The same Rust `glass()` nodes fall back to `UIBlurEffect` on iOS 16â€“25. When Reduce Transparency is enabled, they render as an opaque panel instead. The app does not set `UIDesignRequiresCompatibility`, so system components on iOS 26 are free to adopt the new design as well.

## What is implemented

- C ABI (`cheetah_new`, `cheetah_resize`, `cheetah_event`, `cheetah_free`) over the shared Rust core.
- Ops callback posts JSON to the main queue.
- CoreText text measurement callback on the actor thread.
- Native `UILabel`, custom `UIControl` buttons, `UITextField`, and `UIScrollView` views driven by absolute frames.
- Glass containers, vibrancy through `UIVisualEffectView.contentView`, blur/opaque fallbacks, and interactive glass controls.
- Overlay header/composer with scroll content insets, so messages scroll beneath the chrome.
- Stable accessibility identifiers for Rust-driven controls.
- Variable-height list slots, scroll callbacks, scroll-to-bottom, connection status, and offline outbox from the shared core.

## Verification

CI runs on macOS 26 with Xcode 26, builds the Rust XCFramework, builds both device and simulator targets, launches the app in an iOS 26 simulator, publishes the screenshot above, and runs a UI test that types a message and taps Send. The app has not yet been run on a physical device.
