# Cheetah

Rust runtime that drives native Android and iOS views with no JavaScript.

The runtime owns state, layout, and diffs; the Kotlin and Swift hosts realize
native views and platform APIs. Apps are compiled into the runtime, so every
screen and native call is plain Rust plus the platform implementation.

## Native APIs

Declarative views flow through the diff. Imperative platform calls (haptics,
location, sharing, clipboard, and similar services) use the same op frame and
are routed to `NativeApis` on each host. See
[`docs/native-apis.md`](docs/native-apis.md) for the protocol and how to add a
module.
