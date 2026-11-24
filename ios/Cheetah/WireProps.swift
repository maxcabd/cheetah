import CoreGraphics
import Foundation

/// A property, by its number in the op format (crates/runtime/src/wire.rs, in
/// the same order).
enum Prop: UInt16, CaseIterable {
    case text, size, color, bg, pad, padH, padV, padBottom, radius, weight, width, height
    case bold, font, lineHeight, lines, borderWidth, borderColor, opacity, shadow, clip
    case animate, wrap, placeholderColor, align, placeholder, value, label, testId, gap
    case alignItems, justify, absolute, top, left, contentHeight, scrollTo, glassStyle
    case glassTint, glassInteractive, contentInsetTop, contentInsetBottom, bottom, mapLatitude
    case mapLongitude, mapLatitudeDelta, mapLongitudeDelta, mapMarkers, mapDark, mapShowsUser
    case paged, scrollX, src, fit, contentWidth, pressable, native, nativeProps, translateX
    case translateY, scale, rotate, transitionMs, spring, gestures, refresh, refreshing
    case pageScale

    /// By number, so a name isn't looked up by reflection each time.
    static let names: [String] = allCases.map { "\($0)" }
}

enum PropValue {
    case string(String)
    case number(Float)
    case bool(Bool)
    case count(UInt32)

    /// As native modules get it.
    var object: Any {
        switch self {
        case .string(let value): return value
        case .number(let value): return NSNumber(value: value)
        case .bool(let value): return value
        case .count(let value): return NSNumber(value: value)
        }
    }
}

/// A view's properties as decoded: a short list searched by number, with no
/// dictionary, boxed number or property name for each one.
struct WireProps {
    private var items: [(prop: Prop, value: PropValue)] = []

    mutating func reserve(_ count: Int) {
        items.reserveCapacity(count)
    }

    /// Add a property known not to be here yet.
    mutating func append(_ prop: Prop, _ value: PropValue) {
        items.append((prop, value))
    }

    mutating func set(_ prop: Prop, _ value: PropValue) {
        if let index = items.firstIndex(where: { $0.prop == prop }) {
            items[index].value = value
        } else {
            items.append((prop, value))
        }
    }

    mutating func merge(_ changes: WireProps) {
        for item in changes.items {
            set(item.prop, item.value)
        }
    }

    subscript(_ prop: Prop) -> PropValue? {
        for item in items where item.prop == prop {
            return item.value
        }
        return nil
    }

    func has(_ prop: Prop) -> Bool {
        self[prop] != nil
    }

    func string(_ prop: Prop) -> String? {
        if case .string(let value)? = self[prop] {
            return value
        }
        return nil
    }

    func number(_ prop: Prop) -> CGFloat? {
        switch self[prop] {
        case .number(let value)?: return CGFloat(value)
        case .count(let value)?: return CGFloat(value)
        case .bool(let value)?: return value ? 1 : 0
        default: return nil
        }
    }

    func flag(_ prop: Prop) -> Bool? {
        switch self[prop] {
        case .bool(let value)?: return value
        case .number(let value)?: return value != 0
        case .count(let value)?: return value != 0
        default: return nil
        }
    }

    func bool(_ prop: Prop) -> Bool {
        flag(prop) ?? false
    }

    /// For native modules, which take props by name.
    var dictionary: [String: Any] {
        var out: [String: Any] = [:]
        out.reserveCapacity(items.count)
        for item in items {
            out[Prop.names[Int(item.prop.rawValue)]] = item.value.object
        }
        return out
    }
}
