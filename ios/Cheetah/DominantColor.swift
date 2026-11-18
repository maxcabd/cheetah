import UIKit

extension UIImage {
    /// The image's dominant color as "#rrggbb": the most common color once
    /// the image is shrunk and similar colors are grouped, favoring colorful
    /// pixels over near-black, near-white and gray ones (an average would
    /// muddy everything toward brown-gray).
    func dominantColorHex() -> String? {
        let side = 32
        let bytesPerRow = side * 4
        var pixels = [UInt8](repeating: 0, count: side * bytesPerRow)
        guard let cgImage else {
            return nil
        }
        // The context draws into `pixels` only inside this closure, where the
        // pointer is valid.
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress,
                width: side,
                height: side,
                bitsPerComponent: 8,
                bytesPerRow: bytesPerRow,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else {
                return false
            }
            context.interpolationQuality = .medium
            context.draw(cgImage, in: CGRect(x: 0, y: 0, width: side, height: side))
            return true
        }
        guard drawn else {
            return nil
        }

        // Group colors into 4-bit-per-channel buckets; weight each pixel by
        // how colorful it is, and keep running sums to average each bucket.
        var weights = [Double](repeating: 0, count: 4096)
        var sums = [(Double, Double, Double)](repeating: (0, 0, 0), count: 4096)
        for offset in stride(from: 0, to: pixels.count, by: 4) {
            let alpha = Double(pixels[offset + 3]) / 255
            guard alpha > 0.5 else { continue }
            let red = Double(pixels[offset]) / 255 / alpha
            let green = Double(pixels[offset + 1]) / 255 / alpha
            let blue = Double(pixels[offset + 2]) / 255 / alpha
            let high = max(red, green, blue)
            let low = min(red, green, blue)
            let saturation = high > 0 ? (high - low) / high : 0
            var weight = 1 + 3 * saturation
            if high < 0.12 || low > 0.92 {
                weight *= 0.15
            }
            let bucket = (Int(min(red, 1) * 15.99) << 8)
                | (Int(min(green, 1) * 15.99) << 4)
                | Int(min(blue, 1) * 15.99)
            weights[bucket] += weight
            sums[bucket].0 += red * weight
            sums[bucket].1 += green * weight
            sums[bucket].2 += blue * weight
        }
        guard let best = weights.indices.max(by: { weights[$0] < weights[$1] }), weights[best] > 0 else {
            return nil
        }
        let total = weights[best]
        let channel = { (value: Double) in Int((min(max(value / total, 0), 1) * 255).rounded()) }
        return String(
            format: "#%02x%02x%02x",
            channel(sums[best].0),
            channel(sums[best].1),
            channel(sums[best].2)
        )
    }
}
