import UIKit

/// Native photo pager for the place card. The images, gradient bleed and
/// progress dots all live in one UIKit view so the carousel never has to
/// rebuild them from Rust.
final class PhotoPagerView: UIView, UIScrollViewDelegate {
    private let scroll = UIScrollView()
    private let overlay = PagerOverlayView()
    /// By page, made when its photo arrives: a card shows one page, and
    /// loads only its neighbours.
    private var imageViews: [Int: UIImageView] = [:]
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
        // Bundled too: finding, decoding and shrinking a full-size photo took
        // 20 ms or more of the main thread for each card.
        let name = source.replacingOccurrences(of: "asset://", with: "")
        let size = bounds.size == .zero ? CGSize(width: 320, height: 180) : bounds.size
        let pixels = CGSize(
            width: size.width * UIScreen.main.scale,
            height: size.height * UIScreen.main.scale
        )
        Self.decoding.async { [weak self] in
            guard let loaded = UIImage(named: name)
                ?? UIImage(named: (name as NSString).deletingPathExtension)
                ?? UIImage(contentsOfFile: name)
            else {
                return
            }
            // Big enough to fill the page, no bigger.
            let scale = max(pixels.width / loaded.size.width, pixels.height / loaded.size.height, 0.01)
            let image = scale < 1
                ? loaded.preparingThumbnail(of: CGSize(
                    width: loaded.size.width * scale, height: loaded.size.height * scale
                )) ?? loaded
                : loaded.preparingForDisplay() ?? loaded
            Self.cache.setObject(image, forKey: source as NSString)
            DispatchQueue.main.async {
                self?.show(image, page: page, source: source)
            }
        }
    }

    private static let decoding = DispatchQueue(label: "dev.cheetah.photos", qos: .userInitiated)

    /// Put a loaded photo on its page, unless the photos changed meanwhile.
    private func show(_ image: UIImage, page: Int, source: String) {
        guard page < sources.count, sources[page] == source else { return }
        imageView(for: page).image = image
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
        for (index, imageView) in imageViews {
            imageView.frame = pageFrame(index)
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
        imageViews.values.forEach { $0.removeFromSuperview() }
        imageViews.removeAll(keepingCapacity: true)
        scroll.contentOffset = .zero
        overlay.progress = 0
        overlay.pages = pages
        // Laid out with the rest of the frame, not now.
        setNeedsLayout()
    }

    private func pageFrame(_ index: Int) -> CGRect {
        CGRect(x: CGFloat(index) * bounds.width, y: 0, width: bounds.width, height: bounds.height)
    }

    private func imageView(for page: Int) -> UIImageView {
        if let known = imageViews[page] {
            return known
        }
        let imageView = UIImageView(frame: pageFrame(page))
        imageView.contentMode = .scaleAspectFill
        imageView.clipsToBounds = true
        imageView.isUserInteractionEnabled = false
        scroll.addSubview(imageView)
        imageViews[page] = imageView
        return imageView
    }
}

/// The photo's bottom fade and page dots, as layers: nothing is drawn on
/// the main thread, also not while paging, when the dots change each frame.
private final class PagerOverlayView: UIView {
    var pages = 5 {
        didSet {
            if pages != oldValue { rebuildDots() }
        }
    }

    var progress: CGFloat = 0 {
        didSet {
            if progress != oldValue { updateDots() }
        }
    }

    private let fade = CAGradientLayer()
    private var dots: [CALayer] = []
    private static let fadeHeight: CGFloat = 45
    private static let dotRadius: CGFloat = 2.5
    private static let dotSpacing: CGFloat = 11

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        fade.colors = [
            UIColor(hex: "#0b0b0f00").cgColor,
            UIColor(hex: "#0b0b0f59").cgColor,
            UIColor(hex: "#0b0b0fb3").cgColor,
            UIColor(hex: "#1f1f1f").cgColor,
        ]
        fade.locations = [0, 0.5, 0.8, 1]
        layer.addSublayer(fade)
        rebuildDots()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fade.frame = CGRect(
            x: 0, y: bounds.height - Self.fadeHeight,
            width: bounds.width, height: Self.fadeHeight
        )
        placeDots()
        CATransaction.commit()
    }

    private func rebuildDots() {
        dots.forEach { $0.removeFromSuperlayer() }
        dots = pages > 1 ? (0..<pages).map { _ in
            let dot = CALayer()
            dot.backgroundColor = UIColor.white.cgColor
            dot.bounds = CGRect(x: 0, y: 0, width: Self.dotRadius * 2, height: Self.dotRadius * 2)
            dot.cornerRadius = Self.dotRadius
            layer.addSublayer(dot)
            return dot
        } : []
        setNeedsLayout()
    }

    private func placeDots() {
        let total = Self.dotSpacing * CGFloat(pages - 1)
        let startX = bounds.midX - total / 2
        let y = bounds.height - 12
        for (index, dot) in dots.enumerated() {
            dot.position = CGPoint(x: startX + CGFloat(index) * Self.dotSpacing, y: y)
        }
        styleDots()
    }

    private func updateDots() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        styleDots()
        CATransaction.commit()
    }

    /// The dot nearest the page shown grows and brightens.
    private func styleDots() {
        for (index, dot) in dots.enumerated() {
            let distance = abs(progress - CGFloat(index))
            let influence = max(0, min(1, 1 - distance / 0.75))
            let scale = 1 + 0.55 * influence
            dot.transform = CATransform3DMakeScale(scale, scale, 1)
            dot.opacity = Float(0.22 + 0.78 * influence)
        }
    }
}
