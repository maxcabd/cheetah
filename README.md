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

## Nightshade backend

Nightshade searches local sample places by default. To search the real
nightshadex API (`POST /v1/search`), set its URL when building the runtime:

```powershell
$env:NIGHTSHADE_API_URL = "http://10.0.2.2:8000"   # Android emulator -> host
.\build-android.ps1 -Install
```

```sh
NIGHTSHADE_API_URL=http://localhost:8000 ./ios/build-rust.sh   # iOS simulator
```

The demo app (`nightshade-demo`) always uses the sample places.
