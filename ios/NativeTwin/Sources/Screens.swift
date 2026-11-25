import MapKit
import UIKit

/// Map, result cards and search sheet. Shows the demo's results at launch, or
/// the suggestions first with `--suggestions`.
final class MapScreen: UIViewController {
    private let map = MKMapView()
    private let carousel = Carousel()
    private let sheet = SearchSheet()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0x0b / 255, green: 0x0b / 255, blue: 0x0f / 255, alpha: 1)

        map.frame = view.bounds
        map.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        map.accessibilityIdentifier = "twin-map"
        map.preferredConfiguration = MKStandardMapConfiguration(emphasisStyle: .muted)
        map.region = MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: 37.7749, longitude: -122.4194),
            span: MKCoordinateSpan(latitudeDelta: 0.025, longitudeDelta: 0.025)
        )
        view.addSubview(map)

        for (index, symbol) in ["globe.americas.fill", "location"].enumerated() {
            let button = Theme.glassButton(symbol, size: 48)
            button.frame.origin = CGPoint(x: view.bounds.width - 64, y: 95 + CGFloat(index) * 56)
            button.autoresizingMask = .flexibleLeftMargin
            view.addSubview(button)
        }

        carousel.autoresizingMask = [.flexibleWidth, .flexibleTopMargin]
        carousel.onPage = { [weak self] place in self?.focus(place) }
        carousel.onOpen = { [weak self] place, card in self?.open(place, from: card) }
        view.addSubview(carousel)

        sheet.onSuggestion = { [weak self] _ in self?.show(Array(Place.samples.prefix(1))) }
        sheet.onClear = { [weak self] in self?.clear() }
        if ProcessInfo.processInfo.arguments.contains("--busy") {
            stayBusy()
        }
        if ProcessInfo.processInfo.arguments.contains("--suggestions") {
            clear()
        } else {
            show(Place.samples)
        }
    }

    /// An animation that never ends, as some apps have (for `testNeverIdle`).
    private func stayBusy() {
        let dot = UIView(frame: CGRect(x: 0, y: 0, width: 2, height: 2))
        dot.backgroundColor = .white
        view.addSubview(dot)
        UIView.animate(withDuration: 1, delay: 0, options: [.repeat, .autoreverse]) {
            dot.transform = CGAffineTransform(translationX: 20, y: 0)
        }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        if presentedViewController == nil {
            present(sheet, animated: false)
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        carousel.frame = CGRect(
            x: 0,
            y: view.bounds.height - Theme.carouselBottom - Theme.cardHeight - 20,
            width: view.bounds.width,
            height: Theme.cardHeight + 20
        )
    }

    private func show(_ places: [Place]) {
        carousel.places = places
        map.removeAnnotations(map.annotations)
        map.addAnnotations(
            places.map { place in
                let pin = MKPointAnnotation()
                pin.coordinate = CLLocationCoordinate2D(
                    latitude: place.latitude, longitude: place.longitude
                )
                return pin
            }
        )
        if let first = places.first {
            focus(first)
        }
        sheet.showsResults = true
    }

    private func clear() {
        carousel.places = []
        map.removeAnnotations(map.annotations)
        sheet.showsResults = false
    }

    private func focus(_ place: Place) {
        map.setRegion(
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: place.latitude, longitude: place.longitude),
                span: MKCoordinateSpan(latitudeDelta: 0.004, longitudeDelta: 0.004)
            ),
            animated: true
        )
    }

    private func open(_ place: Place, from card: UIView) {
        let page = PlacePage(place: place)
        page.modalPresentationStyle = .fullScreen
        if #available(iOS 18.0, *) {
            page.preferredTransition = .zoom { _ in card }
        }
        sheet.present(page, animated: true)
    }
}

/// The result cards: one per page, narrower than the screen so the next one
/// peeks in, the neighbours scaled down.
final class Carousel: UIView, UIScrollViewDelegate {
    var onPage: ((Place) -> Void)?
    var onOpen: ((Place, UIView) -> Void)?

    private let scroll = UIScrollView()
    private var cards: [PlaceCard] = []
    private var page = 0

    var places: [Place] = [] {
        didSet {
            cards.forEach { $0.removeFromSuperview() }
            cards = places.map { place in
                let card = PlaceCard(place: place)
                card.addGestureRecognizer(Tap { [weak self, weak card] in
                    guard let card else { return }
                    self?.onOpen?(place, card)
                })
                scroll.addSubview(card)
                return card
            }
            page = 0
            scroll.contentOffset = .zero
            isHidden = places.isEmpty
            setNeedsLayout()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        scroll.isPagingEnabled = true
        scroll.clipsToBounds = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.delegate = self
        addSubview(scroll)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let width = bounds.width * Theme.cardFraction
        let pageWidth = width + Theme.cardGap
        scroll.frame = CGRect(
            x: (bounds.width - pageWidth) / 2, y: 0, width: pageWidth, height: bounds.height
        )
        scroll.contentSize = CGSize(width: pageWidth * CGFloat(cards.count), height: bounds.height)
        for (index, card) in cards.enumerated() {
            card.transform = .identity
            card.frame = CGRect(
                x: CGFloat(index) * pageWidth + Theme.cardGap / 2,
                y: bounds.height - Theme.cardHeight,
                width: width,
                height: Theme.cardHeight
            )
        }
        scaleNeighbours()
    }

    // The peeking cards are outside the scroll view but still scroll it.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        guard hit === self else { return hit }
        return point.y >= bounds.height - Theme.cardHeight ? scroll : nil
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        scaleNeighbours()
    }

    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        let next = Int((scroll.contentOffset.x / max(scroll.bounds.width, 1)).rounded())
        if next != page, places.indices.contains(next) {
            page = next
            UISelectionFeedbackGenerator().selectionChanged()
            onPage?(places[next])
        }
    }

    private func scaleNeighbours() {
        let position = scroll.contentOffset.x / max(scroll.bounds.width, 1)
        for (index, card) in cards.enumerated() {
            let away = min(abs(CGFloat(index) - position), 1)
            let scale = 1 - 0.1 * away
            card.transform = CGAffineTransform(scaleX: scale, y: scale)
        }
    }
}

final class PlaceCard: UIView {
    private let photos = PhotoPager()
    private let pill = UIView()
    private let chevron = Theme.glassButton("chevron.up", size: 38)
    private let actions = ["bookmark", "square.and.arrow.up", "arrow.triangle.turn.up.right.diamond"]
        .map { Theme.glassButton($0, size: 38) }
    private let name: UILabel
    private let line: UIStackView
    private let hours: UIStackView
    private let rating: UIStackView

    init(place: Place) {
        name = Theme.label(place.name, Theme.font("Brice-Bold", 18))
        let medium = Theme.font("Satoshi-Medium", 14)
        line = UIStackView(arrangedSubviews: [
            Theme.label(place.address, medium), Theme.icon("car.fill", 12), Theme.label(place.detail, medium),
        ])
        hours = HoursLine.make(place)
        rating = UIStackView(arrangedSubviews: [
            Theme.icon("g.circle.fill", 14), Theme.label("4.5 (50+)", Theme.font("Satoshi-Medium", 12)),
        ])
        super.init(frame: .zero)
        accessibilityIdentifier = "card-\(place.id)"
        backgroundColor = Theme.card
        layer.cornerRadius = 28
        layer.borderWidth = 2
        layer.borderColor = UIColor(white: 1, alpha: 0.03).cgColor
        clipsToBounds = true
        pill.backgroundColor = UIColor(white: 1, alpha: 0.28)
        pill.layer.cornerRadius = 2.5
        rating.layer.cornerRadius = 15
        rating.layer.borderWidth = 1
        rating.layer.borderColor = UIColor(white: 0.25, alpha: 1).cgColor
        rating.isLayoutMarginsRelativeArrangement = true
        rating.layoutMargins = UIEdgeInsets(top: 0, left: 12, bottom: 0, right: 12)
        for stack in [line, hours, rating] {
            stack.spacing = 6
            stack.alignment = .center
        }
        ([photos, pill, chevron, name, line, hours, rating] + actions).forEach(addSubview)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        let width = bounds.width
        photos.frame = CGRect(x: 0, y: 0, width: width, height: Theme.cardHeader)
        pill.frame = CGRect(x: (width - 36) / 2, y: 8, width: 36, height: 5)
        chevron.frame.origin = CGPoint(x: 16, y: 16)
        for (index, button) in actions.enumerated() {
            button.frame.origin = CGPoint(x: width - 16 - 38 - CGFloat(2 - index) * 46, y: 16)
        }
        var y = Theme.cardHeader + 12
        for (view, height) in [(name, 22), (line, 18), (hours, 18)] as [(UIView, CGFloat)] {
            let fitted = view.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize).width
            view.frame = CGRect(x: 20, y: y, width: min(fitted, width - 40), height: height)
            y += height + 6
        }
        rating.frame = CGRect(x: 20, y: y, width: 110, height: 30)
    }
}

enum HoursLine {
    static func make(_ place: Place) -> UIStackView {
        let medium = Theme.font("Satoshi-Medium", 14)
        return UIStackView(arrangedSubviews: [
            Theme.label(
                place.open ? "Open" : "Closed",
                Theme.font("Satoshi-Bold", 14),
                place.open ? Theme.green : Theme.red
            ),
            Theme.label("·", medium),
            Theme.label(place.hours, medium, place.open ? .white : UIColor(white: 1, alpha: 0.7)),
        ])
    }
}

/// Five pages of the placeholder photo with dots, like PhotoPagerView.
final class PhotoPager: UIView, UIScrollViewDelegate {
    private let scroll = UIScrollView()
    private let dots = UIPageControl()
    private var pages: [UIImageView] = []
    private static let photo = UIImage(named: "second-cup-card-full.jpeg")

    override init(frame: CGRect) {
        super.init(frame: frame)
        clipsToBounds = true
        scroll.isPagingEnabled = true
        scroll.bounces = false
        scroll.showsHorizontalScrollIndicator = false
        scroll.delegate = self
        pages = (0..<5).map { _ in
            let view = UIImageView(image: Self.photo)
            view.contentMode = .scaleAspectFill
            view.clipsToBounds = true
            scroll.addSubview(view)
            return view
        }
        dots.numberOfPages = pages.count
        dots.isUserInteractionEnabled = false
        addSubview(scroll)
        addSubview(dots)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        scroll.frame = bounds
        scroll.contentSize = CGSize(width: bounds.width * CGFloat(pages.count), height: bounds.height)
        for (index, page) in pages.enumerated() {
            page.frame = bounds.offsetBy(dx: CGFloat(index) * bounds.width, dy: 0)
        }
        dots.frame = CGRect(x: 0, y: bounds.height - 26, width: bounds.width, height: 20)
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        dots.currentPage = Int((scroll.contentOffset.x / max(bounds.width, 1)).rounded())
    }
}

/// The search sheet: suggestions and the search bar, or just the bar and a
/// clear button over results.
final class SearchSheet: UIViewController {
    var onSuggestion: ((String) -> Void)?
    var onClear: (() -> Void)?

    private static let suggestions = [
        "Cafes within 12 min walk", "Best dessert spots near me", "Top rated brunch spots",
        "Nearby spots that open late night", "Halal food near me", "Cheap eats nearby",
    ]
    private let heading = Theme.label("Ready to discover?", Theme.font("Brice-SemiBold", 20))
    private let grabber = UIView()
    private let chips = UIScrollView()
    private let field = Theme.glass(radius: 26)
    private let input = UITextField()
    private let clear = Theme.glassButton("xmark", size: 44, label: "Clear")

    var showsResults = false {
        didSet {
            loadViewIfNeeded()
            heading.isHidden = showsResults
            chips.isHidden = showsResults
            clear.isHidden = !showsResults
            view.setNeedsLayout()
            sheetPresentationController?.animateChanges {
                sheetPresentationController?.selectedDetentIdentifier =
                    showsResults ? Self.results : Self.idle
            }
        }
    }

    private static let idle = UISheetPresentationController.Detent.Identifier("idle")
    private static let results = UISheetPresentationController.Detent.Identifier("results")
    private static let expanded = UISheetPresentationController.Detent.Identifier("expanded")

    init() {
        super.init(nibName: nil, bundle: nil)
        isModalInPresentation = true
        modalPresentationStyle = .pageSheet
        guard let sheet = sheetPresentationController else { return }
        sheet.detents = [
            .custom(identifier: Self.results) { _ in Theme.resultSheet },
            .custom(identifier: Self.idle) { _ in Theme.idleSheet },
            .custom(identifier: Self.expanded) { context in context.maximumDetentValue * 0.92 },
        ]
        sheet.largestUndimmedDetentIdentifier = Self.expanded
        sheet.preferredCornerRadius = 54
        sheet.prefersScrollingExpandsWhenScrolledToEdge = false
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        if #available(iOS 26.0, *) {
            view.backgroundColor = .clear
        } else {
            view.backgroundColor = Theme.sheet
        }
        grabber.backgroundColor = UIColor(white: 0.25, alpha: 1)
        grabber.layer.cornerRadius = 3
        input.placeholder = "What are you in the mood for?"
        input.font = Theme.font("Satoshi-Medium", 16)
        input.textColor = .white
        field.contentView.addSubview(input)
        clear.addGestureRecognizer(Tap { [weak self] in self?.onClear?() })
        chips.showsHorizontalScrollIndicator = false
        for title in Self.suggestions {
            let chip = Theme.glass(radius: 18)
            let label = Theme.label(title, Theme.font("Satoshi-Bold", 14))
            label.textAlignment = .center
            label.sizeToFit()
            chip.frame.size = CGSize(width: label.frame.width + 32, height: 42)
            label.frame = chip.bounds
            chip.contentView.addSubview(label)
            chip.isAccessibilityElement = true
            chip.accessibilityTraits = .button
            chip.accessibilityLabel = title
            chip.addGestureRecognizer(Tap { [weak self] in self?.onSuggestion?(title) })
            chips.addSubview(chip)
        }
        [grabber, heading, chips, field, clear].forEach(view.addSubview)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let width = view.bounds.width
        grabber.frame = CGRect(x: (width - 30) / 2, y: 9, width: 30, height: 6)
        heading.frame = CGRect(x: 24, y: 24, width: width - 48, height: 26)
        chips.frame = CGRect(x: 0, y: 64, width: width, height: 160)
        var point = CGPoint(x: 24, y: 8)
        for chip in chips.subviews where chip is UIVisualEffectView {
            if point.x + chip.frame.width > 624 {
                point = CGPoint(x: 24, y: point.y + 52)
            }
            chip.frame.origin = point
            point.x += chip.frame.width + 10
        }
        chips.contentSize = CGSize(width: 648, height: 160)
        let barY = showsResults ? CGFloat(28) : CGFloat(236)
        let trailing: CGFloat = showsResults ? 56 : 0
        field.frame = CGRect(x: 24, y: barY, width: width - 48 - trailing, height: 52)
        input.frame = field.bounds.insetBy(dx: 20, dy: 2)
        clear.frame.origin = CGPoint(x: width - 24 - 44, y: barY + 4)
    }
}

/// A place's page: photo into a blur, details and actions.
final class PlacePage: UIViewController {
    private let place: Place

    init(place: Place) {
        self.place = place
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        overrideUserInterfaceStyle = .dark
        view.backgroundColor = Theme.card
        let width = view.bounds.width

        let photo = PhotoPager(frame: CGRect(x: 0, y: 0, width: width, height: 320))
        let blur = UIVisualEffectView(effect: UIBlurEffect(style: .dark))
        blur.frame = CGRect(x: 0, y: 170, width: width, height: 150)
        let mask = CAGradientLayer()
        mask.frame = blur.bounds
        mask.colors = [UIColor.clear.cgColor, UIColor.black.cgColor]
        let maskView = UIView(frame: blur.bounds)
        maskView.layer.addSublayer(mask)
        blur.mask = maskView
        let fade = CAGradientLayer()
        fade.frame = CGRect(x: 0, y: 230, width: width, height: 90)
        fade.colors = [0, 0.25, 0.65].map { Theme.card.withAlphaComponent($0).cgColor }
        fade.locations = [0, 0.6, 1]
        view.addSubview(photo)
        view.addSubview(blur)
        view.layer.addSublayer(fade)

        let medium = Theme.font("Satoshi-Medium", 14)
        let address = UIStackView(arrangedSubviews: [
            Theme.icon("mappin.and.ellipse", 14, Theme.muted), Theme.label(place.address, medium),
        ])
        address.spacing = 8
        let hours = HoursLine.make(place)
        hours.spacing = 4
        let stack = UIStackView(arrangedSubviews: [
            Theme.label(place.name, Theme.font("Brice-Bold", 26)),
            Theme.label(place.detail, medium, Theme.muted),
            hours, address, actions(),
        ])
        stack.axis = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        stack.frame = CGRect(x: 20, y: 338, width: width - 40, height: 190)
        view.addSubview(stack)

        let close = Theme.glassButton("chevron.down", size: 44, label: "Close")
        close.frame.origin = CGPoint(x: 16, y: 67)
        close.addGestureRecognizer(Tap { [weak self] in self?.dismiss(animated: true) })
        view.addSubview(close)
    }

    private func actions() -> UIView {
        let directions = Theme.glass(radius: 24)
        let label = Theme.label("Directions", Theme.font("Satoshi-Bold", 15))
        label.textAlignment = .center
        label.frame = CGRect(x: 0, y: 0, width: 220, height: 48)
        directions.contentView.addSubview(label)
        directions.widthAnchor.constraint(equalToConstant: 220).isActive = true
        directions.heightAnchor.constraint(equalToConstant: 48).isActive = true
        let others = ["square.and.arrow.up", "bookmark"].map { symbol -> UIView in
            let button = Theme.glassButton(symbol, size: 48)
            button.widthAnchor.constraint(equalToConstant: 48).isActive = true
            button.heightAnchor.constraint(equalToConstant: 48).isActive = true
            return button
        }
        let row = UIStackView(arrangedSubviews: [directions] + others)
        row.spacing = 10
        return row
    }
}
