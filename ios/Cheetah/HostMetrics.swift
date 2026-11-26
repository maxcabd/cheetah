import QuartzCore
import UIKit
import os

/// What a frame cost on this device: applying its ops, by kind of work, and
/// the layout and drawing UIKit does right after. Signposts ("apply",
/// "commit" in Instruments, and in CheetahPerformanceTests) are always on and
/// cost next to nothing; the report goes to the dev server, for
/// `cheetah metrics`.
final class HostMetrics {
    /// Kinds of work inside a frame. The first five are ops; the rest happen
    /// inside them and aren't counted there: putting views on screen, glass,
    /// native modules, native calls (haptics, http) and the map's pins.
    enum Phase: Int, CaseIterable {
        case create, props, children, layout, remove, insert, glass, modules, calls, map
    }

    private static let signposter = OSSignposter(subsystem: "dev.cheetah.host", category: "frames")
    /// With CHEETAH_FRAME_LOG=1 (the performance tests): each frame's report
    /// in the system log, where CI reads it; there is no dev server there.
    private static let reports: Logger? = ProcessInfo.processInfo.environment["CHEETAH_FRAME_LOG"] == "1"
        ? Logger(subsystem: "dev.cheetah.host", category: "report")
        : nil

    private var spent = [UInt64](repeating: 0, count: Phase.allCases.count)
    /// Open phases, innermost last: the phase, when it began, and how much of
    /// it went to phases inside it.
    private var running: [(phase: Phase, start: UInt64, inner: UInt64)] = []
    private var ops = 0
    /// Time by name ("call http.request", "module icon"), within its phase.
    private var named: [String: UInt64] = [:]
    /// The same for work put off until after a frame's ops, and how much of
    /// it ran before that frame's commit, which would otherwise count it.
    private var putOff: [String: UInt64] = [:]
    private var beforeCommit: UInt64 = 0
    private var committing = false

    @inline(__always)
    func measure<T>(_ phase: Phase, named name: String? = nil, _ work: () -> T) -> T {
        running.append((phase, DispatchTime.now().uptimeNanoseconds, 0))
        let result = work()
        // Gone if a frame began inside this one (a nested run loop): the
        // numbers are off then, but nothing breaks.
        guard let done = running.popLast() else { return result }
        let now = DispatchTime.now().uptimeNanoseconds
        let total = now - min(done.start, now)
        let own = total - min(done.inner, total)
        spent[phase.rawValue] += own
        if let name {
            named[name, default: 0] += own
        }
        if let outer = running.indices.last {
            running[outer].inner += total
        }
        return result
    }

    /// Work put off until after the ops (haptics). What runs once the frame
    /// is committed goes in the next frame's report.
    func later(_ name: String, _ work: () -> Void) {
        let start = DispatchTime.now().uptimeNanoseconds
        work()
        let spent = DispatchTime.now().uptimeNanoseconds - start
        putOff[committing ? name : "late \(name)", default: 0] += spent
        if committing {
            beforeCommit += spent
        }
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
        named.removeAll(keepingCapacity: true)
        beforeCommit = 0
        committing = false

        let start = DispatchTime.now().uptimeNanoseconds
        let interval = Self.signposter.beginInterval("apply")
        apply()
        Self.signposter.endInterval("apply", interval)
        let applied = DispatchTime.now().uptimeNanoseconds

        committing = true
        let names = named

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
        ) { [weak self] _, _ in
            Self.signposter.endInterval("commit", commit)
            let total = DispatchTime.now().uptimeNanoseconds - applied
            let after = min(self?.beforeCommit ?? 0, total)
            report["commit_us"] = Self.micros(total - after)
            report["after_us"] = Self.micros(after)
            if let self {
                report["named"] = names.merging(self.putOff, uniquingKeysWith: +)
                    .mapValues(Self.micros)
                self.putOff.removeAll(keepingCapacity: true)
                self.beforeCommit = 0
                self.committing = false
            }
            if let reports = Self.reports,
               let data = try? JSONSerialization.data(withJSONObject: report),
               let line = String(data: data, encoding: .utf8)
            {
                reports.log("frame \(line, privacy: .public)")
            }
            done(report)
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
    }

    private static func micros(_ nanoseconds: UInt64) -> Int {
        Int(nanoseconds / 1000)
    }
}
