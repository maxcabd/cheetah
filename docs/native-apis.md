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

Android and iOS expose the same modules.

| Module | Methods | Result |
| --- | --- | --- |
| `haptics` | `light`, `medium`, `heavy`, `selection`, `success`, `warning`, `error` | fire-and-forget |
| `location` | `request`, `current` | `native` event with `ok`, `latitude`, `longitude`, `accuracy`, or `error` |
| `system` | `openUrl`, `share`, `clipboard.set`, `clipboard.get`, `toast` (Android), `info` | `clipboard`/`info` return a `native` event |
| `secureStore` | `get`, `set`, `delete` | `secureStore.result` with `op`, `key`, `ok`, `value` (`null` if missing), `error` |
| `http` | `request`, `cancel` | `http.response` with `id`, `ok`, `status`, `headers`, `body`, `error` |
| `navigation` | `setCanGoBack` | host sends `navigation.back` on Android Back / iOS left-edge swipe |

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

Useful next modules: notifications, camera/photo picker, WebView, video, and
network status.

## Typed Rust wrappers

Parse any reply with `NativeEvent::parse(&event.payload)`, then hand it to the
module's typed decoder.

| Replaces (Expo/RN) | Rust | iOS | Android |
| --- | --- | --- | --- |
| `expo-secure-store` | `secure_store::{get, set, delete}`, `SecureStoreResult` | Keychain generic password, `AfterFirstUnlockThisDeviceOnly` (`SecureStoreApi.swift`) | AES-256-GCM key in Android Keystore, ciphertext in private prefs (`SecureStoreApi.kt`) |
| `axios` / `fetch` | `http::{HttpClient, HttpRequest, HttpResponse}` | `URLSession` (`HttpApi.swift`) | `HttpURLConnection` (`HttpApi.kt`) |
| `expo-router` (stack) | `nav::Navigator<Route>` | edge-swipe gesture in `NativeApis.swift` | `MainActivity.onBackPressed` → `RustHost.onBackPressed` |
| `AsyncStorage` | `kv::KvStore` (redb, synchronous) | none | none |

```rust
use runtime::{http::{HttpClient, HttpResponse}, nav::Navigator, secure_store, NativeEvent};

// Startup: load the token, call the API with it.
self.calls.push(secure_store::get("access_token"));
let request = self.api.get("/places/search?q=sushi");
self.pending_search = Some(request.id);
self.calls.push(request.into_call());

// App::update for event.kind == "native":
let Some(native) = NativeEvent::parse(&event.payload) else { return };
if self.nav.handle_native(&native) { return; }
if let Some(response) = HttpResponse::from_event(&native) {
    if Some(response.id) == self.pending_search && response.is_success() {
        self.places = response.json().unwrap_or_default();
    }
}

// App::take_native_calls: keep the host's back affordance in sync.
let mut calls = std::mem::take(&mut self.calls);
calls.extend(self.nav.take_sync());
calls
```

Notes:

- `http` bodies are UTF-8 text. `ok` is transport success; check
  `is_success()` for 2xx. Transport errors: `timeout`, `network`,
  `invalid_url`, `cancelled` (Android may also report `unsupported_method`).
- `secureStore` calls are processed in order on a serial queue on both hosts.
- The navigator does not animate screen transitions yet; the swipe triggers
  back on release rather than tracking the finger.
