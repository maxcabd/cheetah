# cheetah-drive

A small, local driver for Cheetah apps. It gives the development loop one
stable vocabulary instead of a pile of adb/XCTest incantations.

The Android adapter is complete. The iOS adapter currently covers launch,
terminate, and screenshots; tree/tap/type on iOS stay in the XCUITest runner
until a small host-side accessibility bridge is added.

## Android loop

```powershell
cargo run -p cheetah-drive -- android start nightshade
cargo run -p cheetah-drive -- android tree
cargo run -p cheetah-drive -- android tap Coffee
cargo run -p cheetah-drive -- android type search "sushi"
cargo run -p cheetah-drive -- android screenshot target/drive/nightshade.png
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
tap "Coffee"
wait 400
expect text "Results for \"coffee\""
expect exists "Clear"
screenshot target/drive/nightshade-search.png
tap "Clear"
expect exists "Open now"
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
- `screenshot <path>`

Run the checked-in example:

```powershell
cargo run -p cheetah-drive -- android test tools/drive/nightshade.drive
```

## iOS

On a Mac with Xcode and a booted simulator:

```sh
cargo run -p cheetah-drive -- ios launch com.example.cheetah --nightshade
cargo run -p cheetah-drive -- ios screenshot target/drive/nightshade-ios.png
cargo run -p cheetah-drive -- ios terminate com.example.cheetah
```

The next iOS adapter step is an XCUITest command loop that reads a drive file
and emits the same JSON tree.
