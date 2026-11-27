import UIKit

/// Reports `.gestures("pan,swipe,long_press,pinch,double_tap")` as `gesture`
/// events (payload: see runtime::gesture). Distances are points from where
/// the gesture began; velocities are points per second.
final class GestureRelay: NSObject, UIGestureRecognizerDelegate {
    private weak var view: UIView?
    private let emit: (String) -> Void
    private(set) var kinds: Set<String> = []
    private var recognizers: [UIGestureRecognizer] = []

    init(view: UIView, emit: @escaping (String) -> Void) {
        self.view = view
        self.emit = emit
        super.init()
    }

    /// Install recognizers for `kinds`; `tap` (the pressable tap) waits for
    /// a double tap to fail when both are present.
    func update(kinds new: Set<String>, tap: UITapGestureRecognizer?) {
        guard new != kinds, let view else {
            return
        }
        recognizers.forEach { view.removeGestureRecognizer($0) }
        recognizers.removeAll()
        kinds = new
        if new.contains("pan") {
            let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
            pan.delegate = self
            add(pan, to: view)
        }
        if new.contains("swipe") {
            let directions: [UISwipeGestureRecognizer.Direction] = [.left, .right, .up, .down]
            for direction in directions {
                let swipe = UISwipeGestureRecognizer(target: self, action: #selector(handleSwipe(_:)))
                swipe.direction = direction
                add(swipe, to: view)
            }
        }
        if new.contains("long_press") {
            add(UILongPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:))), to: view)
        }
        if new.contains("pinch") {
            add(UIPinchGestureRecognizer(target: self, action: #selector(handlePinch(_:))), to: view)
        }
        if new.contains("double_tap") {
            let double = UITapGestureRecognizer(target: self, action: #selector(handleDoubleTap(_:)))
            double.numberOfTapsRequired = 2
            add(double, to: view)
            tap?.require(toFail: double)
        }
        if !new.isEmpty {
            view.isUserInteractionEnabled = true
        }
    }

    private func add(_ recognizer: UIGestureRecognizer, to view: UIView) {
        view.addGestureRecognizer(recognizer)
        recognizers.append(recognizer)
    }

    /// Pinch and pan together (zoom while dragging).
    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer
    ) -> Bool {
        other is UIPinchGestureRecognizer
    }

    private func state(_ recognizer: UIGestureRecognizer) -> String? {
        switch recognizer.state {
        case .began: return "began"
        case .changed: return "changed"
        case .ended: return "ended"
        case .cancelled, .failed: return "cancelled"
        default: return nil
        }
    }

    @objc private func handlePan(_ pan: UIPanGestureRecognizer) {
        guard let state = state(pan), let space = view?.superview ?? view else {
            return
        }
        let moved = pan.translation(in: space)
        let speed = pan.velocity(in: space)
        send("pan", state, dx: moved.x, dy: moved.y, vx: speed.x, vy: speed.y)
    }

    @objc private func handleSwipe(_ swipe: UISwipeGestureRecognizer) {
        let direction: String
        switch swipe.direction {
        case .left: direction = "left"
        case .right: direction = "right"
        case .up: direction = "up"
        default: direction = "down"
        }
        send("swipe", "ended", direction: direction)
    }

    @objc private func handleLongPress(_ press: UILongPressGestureRecognizer) {
        guard press.state == .began else {
            return
        }
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        send("long_press", "ended")
    }

    @objc private func handlePinch(_ pinch: UIPinchGestureRecognizer) {
        guard let state = state(pinch) else {
            return
        }
        send("pinch", state, scale: pinch.scale)
    }

    @objc private func handleDoubleTap(_ tap: UITapGestureRecognizer) {
        send("double_tap", "ended")
    }

    private func send(
        _ type: String,
        _ state: String,
        dx: CGFloat = 0,
        dy: CGFloat = 0,
        vx: CGFloat = 0,
        vy: CGFloat = 0,
        scale: CGFloat = 1,
        direction: String = ""
    ) {
        let payload: [String: Any] = [
            "type": type,
            "state": state,
            "dx": Double(dx),
            "dy": Double(dy),
            "vx": Double(vx),
            "vy": Double(vy),
            "scale": Double(scale),
            "direction": direction,
        ]
        guard
            let data = try? JSONSerialization.data(withJSONObject: payload),
            let json = String(data: data, encoding: .utf8)
        else {
            return
        }
        emit(json)
    }
}

/// `drag="x"` with `drag_snaps="0,321"`: the view follows a finger along x
/// and springs to a snap when let go, all here, at the screen's own rate,
/// with no trip to Rust for each move. Rust hears `drag` when a drag starts
/// (to get ready: show what's underneath) and `snap` ({"index","value"})
/// where it came to rest, and then sets `translate_x` to that value.
///
/// At the first snap, with `drag_edge`, only a drag from within that many
/// points of the view's leading edge moves it (a screen-edge drag, so the
/// map or list under it keeps its own drags); elsewhere a drag anywhere on
/// it does. Distances are measured in its superview, which doesn't move.
final class DragRelay: NSObject, UIGestureRecognizerDelegate {
    private weak var view: UIView?
    private let emit: (_ kind: String, _ payload: String) -> Void
    private var snaps: [CGFloat] = [0]
    private var edge: CGFloat?
    private var start: CGFloat = 0
    private lazy var pan: UIPanGestureRecognizer = {
        let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
        pan.delegate = self
        return pan
    }()
    /// A finger has it, or it's springing to a snap: Rust's `translate_x`
    /// waits until it's at rest.
    private(set) var moving = false

    init(view: UIView, emit: @escaping (_ kind: String, _ payload: String) -> Void) {
        self.view = view
        self.emit = emit
        super.init()
        view.addGestureRecognizer(pan)
    }

    func update(snaps raw: String, edge: CGFloat?) {
        let parsed = raw.split(separator: ",").compactMap { Double($0.trimmingCharacters(in: .whitespaces)) }
        snaps = parsed.isEmpty ? [0] : parsed.map { CGFloat($0) }.sorted()
        self.edge = edge
    }

    func remove() {
        view?.removeGestureRecognizer(pan)
    }

    private var position: CGFloat {
        view?.transform.tx ?? 0
    }

    private func move(to x: CGFloat) {
        guard let view else { return }
        var transform = view.transform
        transform.tx = x
        view.transform = transform
    }

    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let view, let pan = gesture as? UIPanGestureRecognizer else {
            return true
        }
        // Sideways only, so vertical drags (sheets, lists) aren't taken.
        let speed = pan.velocity(in: view.superview)
        guard abs(speed.x) > abs(speed.y) else {
            return false
        }
        // At rest on the first snap, only from the leading edge.
        if let edge, abs(position - (snaps.first ?? 0)) < 1 {
            let touch = pan.location(in: view.superview)
            return touch.x - view.frame.minX <= edge
        }
        return true
    }

    /// The map under it pans too, unless this drag has started.
    func gestureRecognizer(
        _ gesture: UIGestureRecognizer,
        shouldBeRequiredToFailBy other: UIGestureRecognizer
    ) -> Bool {
        other.view?.isDescendant(of: view ?? UIView()) == true
    }

    @objc private func handlePan(_ pan: UIPanGestureRecognizer) {
        guard let view else { return }
        let space = view.superview ?? view
        let lowest = snaps.first ?? 0
        let highest = snaps.last ?? 0
        switch pan.state {
        case .began:
            view.layer.removeAllAnimations()
            start = position
            moving = true
            emit("drag", #"{"state":"began"}"#)
        case .changed:
            let x = start + pan.translation(in: space).x
            // Past the ends it gives, as iOS does: a third of the way.
            let held = x < lowest ? lowest + (x - lowest) / 3 : x > highest ? highest + (x - highest) / 3 : x
            move(to: held)
        case .ended, .cancelled, .failed:
            let speed = pan.velocity(in: space).x
            let here = position
            let index: Int
            if abs(speed) >= 400 {
                // A flick goes the way it was flicked: the next snap along.
                let ahead = speed > 0
                    ? snaps.firstIndex { $0 > here + 1 }
                    : snaps.lastIndex { $0 < here - 1 }
                index = ahead ?? (speed > 0 ? snaps.count - 1 : 0)
            } else {
                index = snaps.indices.min { abs(snaps[$0] - here) < abs(snaps[$1] - here) } ?? 0
            }
            let target = snaps[index]
            let distance = target - here
            UIView.animate(
                withDuration: 0.45,
                delay: 0,
                usingSpringWithDamping: 0.9,
                initialSpringVelocity: abs(distance) > 1 ? speed / distance : 0,
                options: [.allowUserInteraction, .beginFromCurrentState]
            ) {
                self.move(to: target)
            } completion: { _ in
                self.moving = false
            }
            emit("snap", #"{"index":\#(index),"value":\#(Double(target))}"#)
        default:
            break
        }
    }
}
