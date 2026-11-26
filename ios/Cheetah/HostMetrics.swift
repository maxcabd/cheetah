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

    /// The metrics of the frame being applied, for code deep inside it.
    private(set) static weak var active: HostMetrics?

    /// Drawing in Cheetah's own views ("text", "button"), which UIKit does
    /// inside the commit, and how many were drawn: only with the frame log.
    private static var drawing: [String: UInt64] = [:]
    private static var drawCounts: [String: Int] = [:]

    /// Whether views time their drawing (the frame log is on).
    static var timesDrawing: Bool {
        reports != nil
    }

    /// Time a view's drawing by kind, for the commit's share of the report.
    static func drawn(_ name: String, _ work: () -> Void) {
        guard reports != nil else { return work() }
        let start = DispatchTime.now().uptimeNanoseconds
        work()
        drawing[name, default: 0] += DispatchTime.now().uptimeNanoseconds - start
        drawCounts[name, default: 0] += 1
    }

    /// Time `work` by name if a frame is being applied, else just run it.
    static func measure<T>(_ phase: Phase, named name: String, _ work: () -> T) -> T {
        guard let active else { return work() }
        return active.measure(phase, named: name, work)
    }

    /// Laid out by UIKit before the commit (see `frame`): `uikit_layout_us`.
    var layoutRoot: (() -> UIView?)?

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
        Self.drawing.removeAll(keepingCapacity: true)
        Self.drawCounts.removeAll(keepingCapacity: true)

        let start = DispatchTime.now().uptimeNanoseconds
        let interval = Self.signposter.beginInterval("apply")
        Self.active = self
        apply()
        Self.active = nil
        Self.signposter.endInterval("apply", interval)
        let applied = DispatchTime.now().uptimeNanoseconds
        // UIKit would lay out in the commit anyway; done here it can be told
        // apart from drawing.
        var layoutParts: [String: Int] = [:]
        if let root = layoutRoot?() {
            if Self.reports != nil {
                layoutParts = Self.layOutByPart(root)
            } else {
                root.layoutIfNeeded()
            }
        }
        let laidOut = DispatchTime.now().uptimeNanoseconds

        committing = true
        let names = named

        var report: [String: Any] = [
            "bytes": bytes,
            "ops": ops,
            "queue_us": Self.micros(start - min(received, start)),
            "apply_us": Self.micros(applied - start),
            "uikit_layout_us": Self.micros(laidOut - applied),
        ]
        if !layoutParts.isEmpty {
            report["layout_parts"] = layoutParts
        }
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
            let total = DispatchTime.now().uptimeNanoseconds - laidOut
            let after = min(self?.beforeCommit ?? 0, total)
            report["commit_us"] = Self.micros(total - after)
            report["after_us"] = Self.micros(after)
            if let self {
                report["named"] = names.merging(self.putOff, uniquingKeysWith: +)
                    .mapValues(Self.micros)
                self.putOff.removeAll(keepingCapacity: true)
                if !Self.drawing.isEmpty {
                    report["drawn"] = Self.drawing.mapValues(Self.micros)
                    report["drawn_count"] = Self.drawCounts
                    Self.drawing.removeAll(keepingCapacity: true)
                    Self.drawCounts.removeAll(keepingCapacity: true)
                }
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

    /// Lay out `root` a part at a time, deepest first, timing each: which
    /// part of the screen UIKit's layout goes to. Parts are named by their
    /// test id, or their kind of view; only the frame log asks for this.
    private static func layOutByPart(_ root: UIView) -> [String: Int] {
        var parts: [String: Int] = [:]
        var levels: [[UIView]] = [[root]]
        while levels.count < 40 {
            let next = levels.last!.flatMap(\.subviews)
            if next.isEmpty { break }
            levels.append(next)
        }
        for level in levels.reversed() {
            for view in level {
                let start = DispatchTime.now().uptimeNanoseconds
                view.layoutIfNeeded()
                let spent = micros(DispatchTime.now().uptimeNanoseconds - start)
                if spent >= 50 {
                    parts[part(view), default: 0] += spent
                }
            }
        }
        let count = levels.dropFirst().reduce(0) { $0 + $1.count }
        parts["views"] = count
        return parts
    }

    /// The nearest named view, and the kind of view inside it that took the
    /// time ("card-java PhotoPagerView"): a card is many kinds of view.
    private static func part(_ view: UIView) -> String {
        let kind = String(describing: type(of: view))
        var current: UIView? = view
        while let candidate = current {
            if let id = candidate.accessibilityIdentifier, !id.isEmpty {
                return candidate === view ? id : "\(id) \(kind)"
            }
            current = candidate.superview
        }
        return kind
    }

    private static func micros(_ nanoseconds: UInt64) -> Int {
        Int(nanoseconds / 1000)
    }
}
