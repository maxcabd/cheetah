import QuartzCore
import UIKit
import os

/// What a frame cost on this device: applying its ops, by kind of work, and
/// the layout and drawing UIKit does right after. Signposts ("apply",
/// "commit" in Instruments, and in CheetahPerformanceTests) are always on and
/// cost next to nothing; the report goes to the dev server, for
/// `cheetah metrics`.
final class HostMetrics {
    /// Kinds of work inside a frame. `map` and `native` are parts of
    /// whichever op they happen in and aren't counted there.
    enum Phase: Int, CaseIterable {
        case create, props, children, layout, remove, native, map
    }

    private static let signposter = OSSignposter(subsystem: "dev.cheetah.host", category: "frames")

    private var spent = [UInt64](repeating: 0, count: Phase.allCases.count)
    /// Open phases, innermost last: the phase, when it began, and how much of
    /// it went to phases inside it.
    private var running: [(phase: Phase, start: UInt64, inner: UInt64)] = []
    private var ops = 0

    @inline(__always)
    func measure<T>(_ phase: Phase, _ work: () -> T) -> T {
        running.append((phase, DispatchTime.now().uptimeNanoseconds, 0))
        let result = work()
        let total = DispatchTime.now().uptimeNanoseconds - running[running.count - 1].start
        let inner = running.removeLast().inner
        spent[phase.rawValue] += total - min(inner, total)
        if !running.isEmpty {
            running[running.count - 1].inner += total
        }
        return result
    }

    func countOp() {
        ops += 1
    }

    /// Apply one frame (main thread). `received` is when its bytes arrived;
    /// `done` gets the report once UIKit has laid out and committed it.
    func frame(bytes: Int, received: UInt64, done: @escaping ([String: Any]) -> Void, apply: () -> Void) {
        for index in spent.indices {
            spent[index] = 0
        }
        running.removeAll(keepingCapacity: true)
        ops = 0

        let start = DispatchTime.now().uptimeNanoseconds
        let interval = Self.signposter.beginInterval("apply")
        apply()
        Self.signposter.endInterval("apply", interval)
        let applied = DispatchTime.now().uptimeNanoseconds

        var report: [String: Any] = [
            "bytes": bytes,
            "ops": ops,
            "queue_us": Self.micros(start - min(received, start)),
            "apply_us": Self.micros(applied - start),
        ]
        for phase in Phase.allCases {
            report["\(phase)_us"] = Self.micros(spent[phase.rawValue])
        }

        // Core Animation commits at the end of this turn of the run loop
        // (order 2,000,000): just after it, UIKit's share is done.
        let commit = Self.signposter.beginInterval("commit")
        let observer = CFRunLoopObserverCreateWithHandler(
            nil,
            CFRunLoopActivity.beforeWaiting.rawValue | CFRunLoopActivity.exit.rawValue,
            false,
            2_000_001
        ) { _, _ in
            Self.signposter.endInterval("commit", commit)
            report["commit_us"] = Self.micros(DispatchTime.now().uptimeNanoseconds - applied)
            done(report)
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
    }

    private static func micros(_ nanoseconds: UInt64) -> Int {
        Int(nanoseconds / 1000)
    }
}
