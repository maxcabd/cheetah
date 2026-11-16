# Cheetah: current state (M0 audit)

> Audited 2026-11-14 against `main` at `ae236bd` **plus uncommitted working-tree
> changes** (native APIs: `crates/runtime/src/native_api.rs`,
> `NativeApis.kt`, `NativeApis.swift`, `docs/native-apis.md`).
>
> Labels: **implemented** = code exists and was exercised by a test or a run
> during this audit, or by CI; **partial** = code exists but has known gaps or
> was not exercised; **proposed** = does not exist.

This document is the baseline for the agent-native roadmap. Write new tooling
against what it says here, not against assumptions about the repo.

## 1. What was actually run

Machine: Windows 10, rustc 1.98.1, JDK 17, Android SDK platform-tools. No
macOS/Xcode; no Android device or emulator attached.

| Command | Result |
|---|---|
| `cargo test --workspace` | pass: runtime 25 unit + 6 integration (`tests/diff.rs`), cheetah-drive 3, bench 0, gateway 0 |
| `cargo test -p runtime --features source-map` | pass: 26 unit (adds `constructors_capture_call_site`) + 6 integration |
| `cargo build -p runtime --target x86_64-linux-android --release --features source-map` | pass (1m 08s) |
| `build-android.ps1` (Gradle APK) | **not run** in this audit |
| `cheetah-drive android test tools/drive/nightshade.drive` | **not run**: no device attached |
| iOS build / UI tests / drive | **not run locally**: needs macOS. CI runs them (§6) |

## 2. Repository inventory

| Area | Location | What it is | Status |
|---|---|---|---|
| Rust runtime | `crates/runtime/` (~6.1k LOC) | Element tree, keyed reconciliation, Taffy layout, binary op wire format, actor thread, FFI | implemented |
| Demo apps | `crates/runtime/src/demo.rs` (`ChatApp`), `nightshade.rs` (`NightshadeApp`) | Compiled into the runtime; selected at launch | implemented |
| iOS host | `ios/Cheetah/*.swift` | UIKit host. C FFI via `CheetahFFI.h`. XcodeGen project (`ios/project.yml`), iOS 16+ | implemented (CI) |
| Android host | `android/app/.../rustui/*.kt` | Android View host (not Compose), JNI via `RustBridge.kt` | implemented, no CI |
| UI driver | `tools/drive` (`cheetah-drive`) | Cross-host tree/tap/type/expect/screenshot + line-based scripts | implemented (iOS in CI), partial (§5) |
| Runtime bench | `tools/bench` | Layout/diff/serialization timings, no host | implemented (CI, informational) |
| Profiling | `tools/profile` | iOS XCTest signpost metrics; Android `gfxinfo` script | implemented |
| Chat gateway | `tools/gateway` | Dev WebSocket echo/broadcast server on `0.0.0.0:9001` for `ChatApp` | implemented |
| Docs | `README.md`, `docs/native-apis.md`, `ios/README.md`, `android/README.md`, tool READMEs | | partial: no API/architecture docs before this file |

## 3. Runtime architecture

```text
App::view(&ViewContext) -> Element            element.rs
        |  tree::build: stable u32 ids from (parent hash, kind, key | index)
        v
UiNode -> flatten -> FlatTree (HashMap<u32, FlatNode>)    tree.rs
        |  diff(previous, next)                           diff.rs
        v
Vec<Op>: Create/Update/UpdateProps/Text/Children/Remove   (+ Layout from Taffy, + Invoke)
        |  wire::encode -> "CHT1" binary frame            wire.rs
        v
Host::emit_ops(bytes)  -> Swift RustHost / Kotlin RustHost realize native views
        ^
native event -> cheetah_event / nativeEvent -> actor Command::Event
             -> Runtime::event -> App::update -> render -> ops
```

Key facts, with references:

- **App model.** `trait App { view, update, take_actions, has_queued, take_native_calls }`
  (`crates/runtime/src/app.rs`). State is owned by the `App` struct; there is
  no separate store or hook system. Apps are compiled into the runtime and
  chosen by string in `actor::spawn` (`"chat"`, `"nightshade"`,
  `"nightshade-demo"`); hosts pass it via the Android intent extra `app`
  (`MainActivity.kt:34`) or iOS launch arguments.
- **Element kinds.** `Column, Row, Text, Button, Scroll, List, Glass, Sheet,
  Map, Image, HScroll, Native, Input, Space` (`element.rs:14`). Builders:
  `column(), row(), text(), button(), input(), …` re-exported from `lib.rs`.
- **Identity.** Node ids are FNV hashes of the parent hash, kind, and either
  `key` or child index (`tree.rs:child_identity`). Keyed nodes keep identity
  across reorders (`tests/diff.rs::keyed_children_keep_identity_when_the_middle_changes`).
  Unkeyed ids are positional and **not stable across code changes**.
- **Diff.** `diff.rs`, ordered creates → updates → children → removes. Sparse
  `UpdateProps` and `Text` fast paths.
- **Layout.** Taffy (`layout.rs`), with host text measurement through
  `Host::measure_text(_batch)` and a 4096-entry cache (`bridge.rs`). Layout is
  skipped for unchanged trees and non-layout prop changes (`app.rs` tests).
- **Threading.** One `cheetah-ui` OS thread owns the `Runtime`; a Tokio
  runtime handles the gateway WebSocket (`actor.rs`). Hosts talk to it through
  an unbounded command channel.
- **FFI surface.** iOS: `cheetah_new, cheetah_new_app, cheetah_resize,
  cheetah_event, cheetah_event_code, cheetah_lifecycle, cheetah_free`
  (`ios.rs`). Android: matching `Java_com_example_rustui_RustBridge_native*`
  (`ffi.rs`).
- **Events.** Numeric codes 1–16 → `click, change, submit, scroll, page,
  focus, blur, marker, host, incoming, ack, fail, flush, connection, select,
  native` (`app.rs:event_kind_from_code`).
- **Native APIs** *(uncommitted)*. `Op::Invoke { module, method, args }`
  appended to a frame; results come back as `native` events. Modules:
  `haptics`, `location`, `system` (`docs/native-apis.md`).
- **Persistence.** `redb` store for chat messages (`store.rs`), used by `ChatApp`.

## 4. Agent-facing capabilities that already exist

The roadmap's M1/M2 are further along than its §0 assumed.

| Capability | Where | Status |
|---|---|---|
| Authored test ids | `Element::test_id()` / `Props::test_id` → wire prop `testId` | implemented |
| Test id → native identity | iOS `accessibilityIdentifier` (`RustHost.swift:1073`); Android accessibility view-id / UIAutomator `resource-id` (`TestIdAccessibility.kt`) | implemented |
| Source locations per node | `source-map` cargo feature, `#[track_caller]` on constructors (`element.rs`); sent in `Create` ops (`wire.rs:put_source`) | implemented |
| Source → native | iOS: `accessibilityHint = "file:line"` (`RustHost.swift:526`). Android: stored on `NodeTag.source`, logged under `CheetahSource` in debug only | partial: Android does not expose it to the driver |
| UI tree snapshot | `cheetah-drive android tree` (UIAutomator XML → JSON: class/text/desc/id/clickable/bounds); `cheetah-drive ios tree` (XCUITest) | implemented, but it reads the **native accessibility tree**, not the Rust tree |
| Semantic selectors | match by testId/accessibility id, then visible text, then substring (`drive/src/main.rs:find`) | implemented |
| Actions | `tap`, `type`, `screenshot`, `layout`, `logs`, `clock`, `settle` | implemented (Android), subset on iOS |
| Assertions | `expect text / exists / missing` | implemented |
| Scripted scenarios | `.drive` files; `tools/drive/nightshade.drive` | implemented; runs in iOS CI |
| Settle detection | two identical accessibility dumps (`settle`) | implemented: heuristic, not revision-based |
| Runtime metrics | `RuntimeMetrics { build_us, diff_us, prime_us, layout_us, nodes, ops, text_requests }` | implemented, not exposed to hosts/tools |

## 5. Gaps against the roadmap

| Roadmap item | Current state | Status |
|---|---|---|
| Versioned protocol / JSON schema (§4) | `cheetah-drive` emits ad-hoc JSON; errors are `String`s; no `protocol_version`, `request_id`, error codes | proposed |
| Snapshot from the **logical** Rust tree (§6, §12) | Runtime holds `previous: FlatTree` + `frames` but exposes no snapshot. Drive scrapes the native a11y tree, so Rust node id, kind, key, and source are lost on Android | proposed |
| Render revision / stale-target detection | No frame or revision counter in `Runtime` (the gateway's `revision` is lifecycle-only) | proposed |
| Dispatch ack vs render observed | Not distinguished. Drive uses `wait`/`settle` | proposed |
| Structured runtime errors | No `catch_unwind` at FFI or actor boundary; a panic on `cheetah-ui` kills the UI thread with no structured report. `drive android logs` returns raw logcat | proposed |
| Build orchestration (`build.start/status`) | `build-android.ps1`, `ios/build-rust.sh` + xcodegen + xcodebuild; no JSON output or diagnostic classification | partial |
| `app.launch/stop` | `drive android start/stop`, `drive ios launch/terminate` | implemented |
| Event/state trace | None | proposed |
| Redaction of sensitive values | No `sensitive` prop; `value` of inputs appears in native tree | proposed |
| Dev-only gating | `source-map` is a feature flag, but **both build scripts always enable it**, including `--release` | partial |
| Android CI | None. Only `bench.yml` (Ubuntu) and iOS 26 / iOS 18.5 workflows | proposed |
| CLI / MCP adapter | `cheetah-drive` is the CLI. No MCP | partial / proposed |
| Starter template / adding a new app | Apps are hard-coded in `actor::spawn`; no app registration API | proposed |

### Proposed crates in the roadmap, mapped to the repo

| Roadmap crate | Existing equivalent | Recommendation |
|---|---|---|
| `test-driver/` | `tools/drive` | Extend it; don't create a new one |
| `cli/` | `tools/drive` binary | Grow `cheetah-drive` into the CLI or rename it later; one binary |
| `inspector/` | None, but `FlatTree` + `frames` in `Runtime` hold all the data | Add a `Runtime::snapshot()` in `runtime` first; split out a crate only if it grows |
| `protocol/` | None | New small crate (serde types only), shared by runtime and drive |
| `devtools/`, `mcp/` | None | Defer until protocol + snapshot land |

## 6. CI status

| Workflow | Runner | Does |
|---|---|---|
| `rust.yml` | ubuntu-latest | `cargo test --workspace --locked` and `cargo test -p runtime --features source-map` (added 2026-11-15) |
| `bench.yml` | ubuntu-latest | `cargo run -p cheetah-bench --release`, posts summary |
| `ios.yml` | macos-26 | Rust XCFramework → xcodegen → device + simulator builds → screenshots → XCUITests → `cheetah-drive ios test nightshade.drive` → perf report; publishes to `ios-preview`, `nightshade-preview`, `ci-logs` branches |
| `ios-fallback.yml` | macos-15, Xcode 16.4 | Same on iOS 18.5 (blur fallback) |

CI status was not checked in this audit; read it from the Actions tab.

## 7. Known limitations and risks found during the audit

1. ~~**Android testId replaces the accessibility label.**~~ Fixed
   2026-11-15: testId is now the accessibility view-id (`resource-id`) via
   `TestIdAccessibility`, and `contentDescription` carries only `label`,
   matching iOS. Not yet verified on a device.
2. **iOS exposes source paths through `accessibilityHint`,** which VoiceOver
   reads aloud, and `source-map` is on in every scripted build. Acceptable for
   a dev build; must not ship.
3. ~~**No `cargo test` in CI.**~~ Fixed 2026-11-15 (`rust.yml`).
4. **`.cargo/config.toml` hard-codes Windows NDK paths**
   (`C:/Users/User/AppData/Local/Android/Sdk/ndk/26.1.10909125/...`). Android
   Rust builds are not reproducible on another machine or Linux CI as-is.
5. **Gateway binds `0.0.0.0:9001`.** Fine for emulator access on a dev box;
   bind to localhost for a devtools endpoint (roadmap §12).
6. **Android package is still `com.example.rustui`**; iOS bundle is
   `com.example.cheetah`. Drive hard-codes both.
7. **Unkeyed node ids are positional.** Agents should rely on `test_id` or
   `key`; the protocol should say generated ids are not stable.

## 8. Suggested next steps (smallest useful order)

1. Make the NDK path configurable (env var or `ANDROID_NDK_HOME`) so
   Android can join CI. (`cargo test` in CI and the testId/label fix are
   done.)
2. Run `nightshade.drive` on an Android emulator to confirm testId lookup
   through `resource-id`.
3. Add a frame `revision: u64` to `Runtime` and a `Runtime::snapshot()`
   returning the logical tree: id, key, kind, testId, label, text, frame,
   source, plus redaction for inputs marked sensitive. Unit-test it in
   `crates/runtime` with no host. This is the roadmap's M1 core, and it
   needs no new transport.
4. Choose a transport for the snapshot (a dev-only host hook that writes JSON
   on request, or a localhost socket on the actor). Then let `cheetah-drive`
   merge the logical snapshot with native bounds.
5. Give `cheetah-drive` `--json` output with `protocol_version`, error
   `code`s, and exit codes. That is the start of the shared protocol crate.
