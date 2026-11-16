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
