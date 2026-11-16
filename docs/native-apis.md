# Native APIs

Cheetah runs app logic in Rust and realizes views with native controls. Some
platform APIs are commands rather than views: haptics, location, sharing,
clipboard access, and similar services.

Those commands use the same op frame as the view tree. The runtime appends an
`invoke` op after the frame's create/update/layout ops:

```text
Invoke { module, method, args }
```

`args` is JSON serialized as a string. The Android and iOS hosts route the call
to `NativeApis`; async APIs answer through the `native` event kind:

```json
{"module":"location","method":"result","ok":true,"latitude":37.77,"longitude":-122.42}
```

## Rust side

Apps queue calls while handling an input event:

```rust
use crate::native_api::NativeCall;

self.calls.push(NativeCall::new("haptics", "light", "{}"));
self.calls.push(NativeCall::json(
    "location",
    "request",
    serde_json::json!({"timeoutMs": 5000}),
));
```

`App::take_native_calls` drains the queue. `Runtime::render` appends the calls
to the frame, including when the view tree itself is unchanged.

## Host side

Android and iOS both expose three modules today.

| Module | Methods | Result |
| --- | --- | --- |
| `haptics` | `light`, `medium`, `heavy`, `selection`, `success`, `warning`, `error` | fire-and-forget |
| `location` | `request`, `current` | `native` event with `ok`, `latitude`, `longitude`, `accuracy`, or `error` |
| `system` | `openUrl`, `share`, `clipboard.set`, `clipboard.get`, `toast` (Android), `info` | `clipboard`/`info` return a `native` event |

Android implementations live in
`android/app/src/main/java/com/example/rustui/NativeApis.kt`. iOS
implementations live in `ios/Cheetah/NativeApis.swift`. Both hosts already
declare the location permission strings and Android declares the location
permissions in the manifest.

## Adding another API

1. Add a method to the platform handler:
   - Android: add a branch in `NativeApis.invoke` and implement it in Kotlin.
   - iOS: add a branch in `NativeApis.invoke` and implement it in Swift.
2. Queue a `NativeCall` from the Rust app.
3. If the API is async, emit a `native` event and handle it in `App::update`.

No wire-format change is needed for a new module or method.

## What “all native APIs” means

There is no reflection switch that exposes every platform SDK safely. Expo and
React Native do not have one either; they ship a module per API. Cheetah now has
the equivalent extension point. Coverage is added module by module.

Useful next modules: notifications, secure storage, camera/photo picker,
WebView, video, and network status.
