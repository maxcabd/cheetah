import UIKit

/// Native photo pager for the place card. The images, gradient bleed and
/// progress dots all live in one UIKit view so the carousel never has to
/// rebuild them from Rust.
final class PhotoPagerView: UIView, UIScrollViewDelegate {
    private let scroll = UIScrollView()
    private let overlay = PagerOverlayView()
    private var imageViews: [UIImageView] = []
    /// One source per page: asset://name or an http(s) URL.
    private var sources: [String] = []
    /// Pages whose photo has been asked for.
    private var requested: Set<Int> = []
    private var pages = 5
    private static let cache = NSCache<NSString, UIImage>()
    /// The first photo's dominant color ("#rrggbb"), once it has loaded.
    var onDominantColor: ((String) -> Void)?

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

    /// One bundled photo repeated over `count` pages (the placeholder).
    func configure(src: String, count: Int) {
        configure(photos: Array(repeating: src, count: max(count, 1)))
    }

    /// A page per photo, loaded as needed: bundled (asset://) or from the
    /// web, cached by URL.
    func configure(photos: [String]) {
        let photos = photos.isEmpty ? [""] : photos
        guard photos != sources else { return }
        sources = photos
        pages = photos.count
        requested.removeAll()
        rebuild()
        loadNear(page: 0)
    }

    /// Load the photo on `page` and its neighbours, each once: a card of five
    /// photos shows one.
    private func loadNear(page: Int) {
        for index in max(page - 1, 0)...max(page + 1, 0)
        where index < sources.count && !sources[index].isEmpty && requested.insert(index).inserted {
            load(sources[index], page: index)
        }
    }

    private func load(_ source: String, page: Int) {
        if let cached = Self.cache.object(forKey: source as NSString) {
            show(cached, page: page, source: source)
            return
        }
        if source.hasPrefix("http://") || source.hasPrefix("https://"), let url = URL(string: source) {
            URLSession.shared.dataTask(with: url) { [weak self] data, _, _ in
                guard let data, let loaded = UIImage(data: data) else { return }
                // Decoded here, not on the main thread when first drawn.
                let image = loaded.preparingForDisplay() ?? loaded
                Self.cache.setObject(image, forKey: source as NSString)
                DispatchQueue.main.async {
                    self?.show(image, page: page, source: source)
                }
            }.resume()
            return
        }
        let name = source.replacingOccurrences(of: "asset://", with: "")
        guard let image = UIImage(named: name)
            ?? UIImage(named: (name as NSString).deletingPathExtension)
            ?? UIImage(contentsOfFile: name)
        else {
            return
        }
        Self.cache.setObject(image, forKey: source as NSString)
        show(image, page: page, source: source)
    }

    /// Put a loaded photo on its page, unless the photos changed meanwhile.
    private func show(_ image: UIImage, page: Int, source: String) {
        guard page < sources.count, sources[page] == source, page < imageViews.count else { return }
        imageViews[page].image = image
        if page == 0 {
            reportDominantColor(image, source: source)
        }
    }

    /// Off the main thread: shrinking and counting colors costs a few ms.
    private func reportDominantColor(_ image: UIImage, source: String) {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let color = image.dominantColorHex() else { return }
            DispatchQueue.main.async {
                guard let self, self.sources.first == source else { return }
                self.onDominantColor?(color)
            }
        }
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
        loadNear(page: Int((scrollView.contentOffset.x / bounds.width).rounded()))
    }

    private func rebuild() {
        imageViews.forEach { $0.removeFromSuperview() }
        imageViews.removeAll(keepingCapacity: true)
        for _ in 0..<pages {
            let imageView = UIImageView()
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
