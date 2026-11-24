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

    static func image(_ symbol: String, size: CGFloat) -> UIImage? {
        let key = "\(symbol)|\(size)"
        if let known = images[key] {
            return known
        }
        let config = UIImage.SymbolConfiguration(pointSize: size, weight: .regular)
        let image = UIImage(systemName: symbol, withConfiguration: config)?
            .withRenderingMode(.alwaysTemplate)
        images[key] = image
        return image
    }
}
