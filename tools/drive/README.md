# cheetah-drive

A small, local driver for Cheetah apps. It gives the development loop one
stable vocabulary instead of a pile of adb/XCTest incantations.

Android uses UIAutomator through adb. iOS uses an XCUITest command runner
(`CheetahDriveTests`) that reads the same drive file and writes a machine-
readable tree. Normal iOS UI test runs skip the runner unless a drive script
is present.

## Android loop

```powershell
cargo run -p cheetah-drive -- android start nightshade
cargo run -p cheetah-drive -- android tree
cargo run -p cheetah-drive -- android tap Coffee
cargo run -p cheetah-drive -- android type search "sushi"
cargo run -p cheetah-drive -- android screenshot target/drive/nightshade.png
cargo run -p cheetah-drive -- android settle
cargo run -p cheetah-drive -- android clock +500
cargo run -p cheetah-drive -- android logs 300
```

`tree` prints the accessibility hierarchy as JSON. Nodes are matched by
`testId`/accessibility id, visible text, or substring, in that order. A
card or chip without a test id can still be driven by its label.

The Rust Android host applies `testId` as the native view's content
description, so the same identifiers used by iOS UI tests are visible to
the driver.

## Scripts

A drive file is a sequence of operations, one per line. Empty lines and
`#` comments are ignored.

```text
start nightshade
wait 1000
tap "Cafes within 12 min walk"
wait 400
expect exists "Ask a follow up"
expect exists "Clear"
screenshot target/drive/nightshade-search.png
tap "Clear"
expect exists "Ready to discover?"
```

Supported operations:

- `start [chat|nightshade|nightshade-demo]`
- `stop`
- `tap <text|id|desc>`
- `type <target> <text>`
- `expect text <text>`
- `expect exists <target>`
- `expect missing <target>`
- `wait <milliseconds>`
- `settle [timeout_ms]` — wait until two accessibility snapshots agree
- `clock +<milliseconds>` or `clock settle` — advance/settle the driver clock
- `screenshot <path>`

Run the checked-in example:

```powershell
cargo run -p cheetah-drive -- android test tools/drive/nightshade.drive
```

## iOS

On a Mac with Xcode, XcodeGen, and a booted simulator:

```sh
# Generate ios/Cheetah.xcodeproj first if it is absent.
cargo run -p cheetah-drive -- ios tree nightshade
cargo run -p cheetah-drive -- ios tap Coffee nightshade
cargo run -p cheetah-drive -- ios test tools/drive/nightshade.drive
cargo run -p cheetah-drive -- ios screenshot target/drive/nightshade-ios.png
```

An XCUITest screenshot operation keeps its image inside the simulator's
temporary directory; use the standalone `ios screenshot` command when the
image needs to land on the Mac.

`ios tree`, `ios tap`, and `ios type` each launch the app through XCTest and
are therefore best for one-shot inspection. Use `ios test` for a stateful
sequence. Override `CHEETAH_IOS_PROJECT` and `CHEETAH_IOS_DESTINATION` if the
project path or simulator name differs.

Source-map metadata is compiled in when the host build uses the
`source-map` feature. Both local host build scripts enable it; production
builds should leave it off.
