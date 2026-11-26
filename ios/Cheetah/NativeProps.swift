import UIKit

/// `native_props` strings, parsed once each: every icon of a kind, and every
/// re-apply, has the same one. Main thread.
enum NativeProps {
    private static var parsed: [String: [String: Any]] = [:]

    static func parse(_ raw: String) -> [String: Any]? {
        if let known = parsed[raw] {
            return known
        }
        guard
            let data = raw.data(using: .utf8),
            let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return nil
        }
        // Photo lists and the like are different each time: don't keep
        // everything forever.
        if parsed.count >= 256 {
            parsed.removeAll(keepingCapacity: true)
        }
        parsed[raw] = config
        return config
    }
}

/// SF Symbols as template images, made once per name and size. Main thread.
enum SymbolImages {
    private static var images: [String: UIImage] = [:]
    private static let lock = NSLock()
    /// The icons ("symbol|size") drawn last time, made again at launch.
    private static let usedKey = "dev.cheetah.symbols"
    private static var used: Set<String> = []
    private static var saving = false

    /// Make the icons the app drew last time before it needs them, off the
    /// main thread: each new symbol cost up to 80 ms of the frame showing it.
    static func warmUp() {
        let keys = UserDefaults.standard.stringArray(forKey: usedKey) ?? []
        used = Set(keys)
        DispatchQueue.global(qos: .userInitiated).async {
            // Drawn, not only looked up: the cost is the first symbol drawn.
            let config = UIImage.SymbolConfiguration(pointSize: 16, weight: .regular)
            if let symbol = UIImage(systemName: "circle", withConfiguration: config) {
                _ = UIGraphicsImageRenderer(size: symbol.size).image { _ in
                    symbol.withTintColor(.white).draw(at: .zero)
                }
            }
            for key in keys {
                let parts = key.split(separator: "|")
                guard parts.count == 2, let size = Double(parts[1]) else { continue }
                lock.lock()
                let known = images[key] != nil
                lock.unlock()
                if known { continue }
                let image = make(String(parts[0]), size: CGFloat(size))
                lock.lock()
                if images[key] == nil { images[key] = image }
                lock.unlock()
            }
        }
    }

    static func image(_ symbol: String, size: CGFloat) -> UIImage? {
        let key = "\(symbol)|\(size)"
        lock.lock()
        let known = images[key]
        lock.unlock()
        if let known {
            return known
        }
        let image = make(symbol, size: size)
        lock.lock()
        images[key] = image
        lock.unlock()
        remember(key)
        return image
    }

    private static func make(_ symbol: String, size: CGFloat) -> UIImage? {
        let config = UIImage.SymbolConfiguration(pointSize: size, weight: .regular)
        return UIImage(systemName: symbol, withConfiguration: config)?
            .withRenderingMode(.alwaysTemplate)
    }

    /// Saved a little later, once for all the icons of a screen.
    private static func remember(_ key: String) {
        guard used.insert(key).inserted, !saving else { return }
        saving = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
            saving = false
            UserDefaults.standard.set(Array(used), forKey: usedKey)
        }
    }
}
