import UIKit

/// Native photo pager for the place card. The images, gradient bleed and
/// progress dots all live in one UIKit view so the carousel never has to
/// rebuild them from Rust.
final class PhotoPagerView: UIView, UIScrollViewDelegate {
    private let scroll = UIScrollView()
    private let overlay = PagerOverlayView()
    private var imageViews: [UIImageView] = []
    private var source = ""
    private var pages = 5
    private var image: UIImage?

    override init(frame: CGRect) {
        super.init(frame: frame)
        clipsToBounds = true
        scroll.isPagingEnabled = true
        scroll.showsHorizontalScrollIndicator = false
        scroll.showsVerticalScrollIndicator = false
        scroll.bounces = false
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.delegate = self
        addSubview(scroll)
        addSubview(overlay)
        overlay.isUserInteractionEnabled = false
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(src: String, count: Int) {
        if src == source, count == pages, image != nil {
            return
        }
        let sourceChanged = src != source
        source = src
        pages = max(count, 1)
        if image == nil || sourceChanged {
            let name = src.replacingOccurrences(of: "asset://", with: "")
            image = UIImage(named: name)
                ?? UIImage(named: (name as NSString).deletingPathExtension)
                ?? UIImage(contentsOfFile: name)
        }
        rebuild()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scroll.frame = bounds
        let width = bounds.width
        let height = bounds.height
        scroll.contentSize = CGSize(width: width * CGFloat(pages), height: height)
        for (index, imageView) in imageViews.enumerated() {
            imageView.frame = CGRect(
                x: CGFloat(index) * width,
                y: 0,
                width: width,
                height: height
            )
        }
        overlay.frame = bounds
        overlay.pages = pages
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        guard bounds.width > 0 else { return }
        overlay.progress = scrollView.contentOffset.x / bounds.width
    }

    private func rebuild() {
        imageViews.forEach { $0.removeFromSuperview() }
        imageViews.removeAll(keepingCapacity: true)
        for _ in 0..<pages {
            let imageView = UIImageView(image: image)
            imageView.contentMode = .scaleAspectFill
            imageView.clipsToBounds = true
            imageView.isUserInteractionEnabled = false
            scroll.addSubview(imageView)
            imageViews.append(imageView)
        }
        overlay.pages = pages
        setNeedsLayout()
        layoutIfNeeded()
        overlay.setNeedsDisplay()
    }
}

private final class PagerOverlayView: UIView {
    var pages = 5
    var progress: CGFloat = 0 {
        didSet { setNeedsDisplay() }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext(), bounds.width > 0 else {
            return
        }
        let gradientHeight = 45.0
        let colors = [
            UIColor(hex: "#0b0b0f00").cgColor,
            UIColor(hex: "#0b0b0f59").cgColor,
            UIColor(hex: "#0b0b0fb3").cgColor,
            UIColor(hex: "#1f1f1f").cgColor,
        ] as CFArray
        let locations: [CGFloat] = [0, 0.5, 0.8, 1]
        if let gradient = CGGradient(
            colorsSpace: CGColorSpaceCreateDeviceRGB(),
            colors: colors,
            locations: locations
        ) {
            let start = CGPoint(x: 0, y: bounds.height - gradientHeight)
            let end = CGPoint(x: 0, y: bounds.height)
            context.drawLinearGradient(
                gradient,
                start: start,
                end: end,
                options: [.drawsBeforeStartLocation, .drawsAfterEndLocation]
            )
        }

        guard pages > 1 else { return }
        let radius: CGFloat = 2.5
        let spacing: CGFloat = 11
        let total = spacing * CGFloat(pages - 1)
        let startX = bounds.midX - total / 2
        let y = bounds.height - 12
        for index in 0..<pages {
            let distance = abs(progress - CGFloat(index))
            let influence = max(0, min(1, 1 - distance / 0.75))
            let scale = 1 + 0.55 * influence
            let dot = UIBezierPath(
                ovalIn: CGRect(
                    x: startX + CGFloat(index) * spacing - radius * scale,
                    y: y - radius * scale,
                    width: radius * 2 * scale,
                    height: radius * 2 * scale
                )
            )
            UIColor.white.withAlphaComponent(0.22 + 0.78 * influence).setFill()
            dot.fill()
        }
    }
}
