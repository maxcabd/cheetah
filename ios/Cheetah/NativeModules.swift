import MapKit
import UIKit

protocol CheetahNativeModule: AnyObject {
    var nativeView: UIView { get }
    func apply(props: [String: Any])
    func layout()
    func destroy()
}

extension CheetahNativeModule {
    func layout() {}
    func destroy() {}
}

/// Container for a semantic native module. The Rust layout owns this box;
/// the module owns the platform control and its internal behavior.
final class NativeModuleHostView: UIView {
    private(set) var module: CheetahNativeModule?
    var onEvent: ((String, String) -> Void)?
    private var pendingName: String?
    private var pendingProps: [String: Any]?
    private var mapScheduled = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(name: String, props: [String: Any]) {
        if module == nil, name == "map",
           bounds.width <= 1 || bounds.height <= 1 || mapScheduled
        {
            pendingName = name
            pendingProps = props
            backgroundColor = UIColor(hex: "#101826")
            setNeedsLayout()
            return
        }
        if module == nil {
            materialize(name: name, props: props)
            return
        }
        module?.apply(props: props)
        if let sheet = module as? SheetNativeModule {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                sheet.presentIfNeeded(owner: self)
            }
        }
        setNeedsLayout()
    }

    private func materialize(name: String, props: [String: Any]) {
        let created = NativeModuleRegistry.make(name: name, owner: self)
        module = created
        if name != "sheet", let native = created?.nativeView {
            addSubview(native)
        }
        created?.apply(props: props)
        if let sheet = created as? SheetNativeModule {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                sheet.presentIfNeeded(owner: self)
            }
        }
        pendingName = nil
        pendingProps = nil
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if module == nil, pendingName == "map", bounds.width > 1, bounds.height > 1,
           !mapScheduled
        {
            mapScheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else {
                    return
                }
                self.mapScheduled = false
                guard self.module == nil, self.bounds.width > 1,
                      self.bounds.height > 1
                else {
                    return
                }
                guard let name = self.pendingName, let props = self.pendingProps else {
                    return
                }
                self.pendingName = nil
                self.pendingProps = nil
                self.materialize(name: name, props: props)
            }
        }
        module?.nativeView.frame = bounds
        module?.layout()
    }

    /// Access hatch for host-side integrations that need the platform object.
    func hatch<T>(_ type: T.Type) -> T? {
        module as? T
    }

    func event(_ kind: String, _ payload: String) {
        onEvent?(kind, payload)
    }

    func destroy() {
        module?.destroy()
        module = nil
        pendingName = nil
        pendingProps = nil
        mapScheduled = false
    }
}

private enum NativeModuleRegistry {
    static func make(name: String, owner: NativeModuleHostView) -> CheetahNativeModule? {
        switch name {
        case "search":
            return SearchNativeModule(owner: owner)
        case "map":
            return MapNativeModule(owner: owner)
        case "sheet":
            return SheetNativeModule(owner: owner)
        case "menu":
            return MenuNativeModule(owner: owner)
        case "icon":
            return IconNativeModule()
        case "gradient":
            return GradientNativeModule()
        case "photo":
            return PhotoPagerNativeModule()
        default:
            return nil
        }
    }
}

private final class PlaceAnnotation: NSObject, MKAnnotation {
    let coordinate: CLLocationCoordinate2D
    let placeID: String
    var title: String?
    var kind: String
    var icon: String
    var iconColor: String
    var selected: Bool

    init(
        coordinate: CLLocationCoordinate2D,
        placeID: String,
        title: String?,
        kind: String,
        icon: String,
        iconColor: String,
        selected: Bool
    ) {
        self.coordinate = coordinate
        self.placeID = placeID
        self.title = title
        self.kind = kind
        self.icon = icon
        self.iconColor = iconColor
        self.selected = selected
    }
}

private final class MapNativeModule: NSObject, CheetahNativeModule, MKMapViewDelegate {
    let map: MKMapView
    private weak var owner: NativeModuleHostView?
    private var pendingRegion: MKCoordinateRegion?
    private var regionApplied = false
    private var appliedAfterLoad = false
    private var lastRegionKey: String?
    private var lastMarkers: String?
    private var lastDark: Bool?
    private var lastShowUser: Bool?
    private var nextRegionAnimated = false
    private var annotationsByID: [String: PlaceAnnotation] = [:]

    var nativeView: UIView {
        map
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        let initialFrame = owner.bounds.width > 0 && owner.bounds.height > 0
            ? owner.bounds
            : CGRect(x: 0, y: 0, width: 1, height: 1)
        map = MKMapView(frame: initialFrame)
        super.init()
        map.backgroundColor = UIColor(hex: "#101826")
        map.isOpaque = true
        map.overrideUserInterfaceStyle = .dark
        map.mapType = .mutedStandard
        map.pointOfInterestFilter = .excludingAll
        map.showsBuildings = false
        map.showsTraffic = false
        map.showsCompass = false
        map.showsScale = false
        map.isRotateEnabled = false
        map.isPitchEnabled = false
        map.delegate = self
    }

    func apply(props: [String: Any]) {
        if let dark = props["mapDark"] as? NSNumber {
            let value = dark.boolValue
            let changed = lastDark.map { $0 != value } ?? true
            if changed {
                map.overrideUserInterfaceStyle = value ? .dark : .light
                lastDark = value
            }
        }

        let showUser = (props["mapShowsUser"] as? NSNumber)?.boolValue ?? false
        let showUserChanged = lastShowUser.map { $0 != showUser } ?? true
        if showUserChanged {
            map.showsUserLocation = showUser
            lastShowUser = showUser
        }

        if let latitude = props["mapLatitude"] as? NSNumber,
           let longitude = props["mapLongitude"] as? NSNumber,
           let latitudeDelta = props["mapLatitudeDelta"] as? NSNumber,
           let longitudeDelta = props["mapLongitudeDelta"] as? NSNumber
        {
            let regionKey = "\(latitude.doubleValue),\(longitude.doubleValue),\(latitudeDelta.doubleValue),\(longitudeDelta.doubleValue)"
            if regionKey != lastRegionKey {
                nextRegionAnimated = lastRegionKey != nil
                lastRegionKey = regionKey
                pendingRegion = MKCoordinateRegion(
                    center: CLLocationCoordinate2D(
                        latitude: latitude.doubleValue,
                        longitude: longitude.doubleValue
                    ),
                    span: MKCoordinateSpan(
                        latitudeDelta: latitudeDelta.doubleValue,
                        longitudeDelta: longitudeDelta.doubleValue
                    )
                )
                regionApplied = false
                appliedAfterLoad = false
                applyPendingRegion()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
                    self?.applyPendingRegion(force: true)
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
                    self?.applyPendingRegion(force: true)
                }
            }
        }

        guard let rawMarkers = props["mapMarkers"] as? String, rawMarkers != lastMarkers else {
            return
        }
        lastMarkers = rawMarkers
        guard
            let data = rawMarkers.data(using: .utf8),
            let markers = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            return
        }

        let samePlaces = markers.count == annotationsByID.count && markers.allSatisfy { marker in
            guard
                let id = marker["id"] as? String,
                let existing = annotationsByID[id],
                let latitude = marker["lat"] as? NSNumber,
                let longitude = marker["lng"] as? NSNumber
            else {
                return false
            }
            return abs(existing.coordinate.latitude - latitude.doubleValue) < 0.0000001
                && abs(existing.coordinate.longitude - longitude.doubleValue) < 0.0000001
        }

        if samePlaces {
            for marker in markers {
                guard
                    let id = marker["id"] as? String,
                    let existing = annotationsByID[id]
                else {
                    continue
                }
                existing.title = marker["name"] as? String
                existing.kind = marker["kind"] as? String ?? "static"
                existing.icon = marker["icon"] as? String ?? "utensils"
                existing.iconColor = marker["iconColor"] as? String ?? "#D32F2F"
                existing.selected = (marker["selected"] as? NSNumber)?.boolValue ?? false
                if let view = map.view(for: existing) as? MKAnnotationView {
                    view.image = Self.markerImage(existing)
                }
            }
            return
        }

        map.removeAnnotations(Array(annotationsByID.values))
        annotationsByID.removeAll(keepingCapacity: true)
        for marker in markers {
            guard
                let latitude = marker["lat"] as? NSNumber,
                let longitude = marker["lng"] as? NSNumber
            else {
                continue
            }
            let annotation = PlaceAnnotation(
                coordinate: CLLocationCoordinate2D(
                    latitude: latitude.doubleValue,
                    longitude: longitude.doubleValue
                ),
                placeID: marker["id"] as? String ?? "",
                title: marker["name"] as? String,
                kind: marker["kind"] as? String ?? "static",
                icon: marker["icon"] as? String ?? "utensils",
                iconColor: marker["iconColor"] as? String ?? "#D32F2F",
                selected: (marker["selected"] as? NSNumber)?.boolValue ?? false
            )
            annotationsByID[annotation.placeID] = annotation
            map.addAnnotation(annotation)
        }
    }

    func layout() {
        applyPendingRegion()
    }

    private func applyPendingRegion(force: Bool = false) {
        guard let region = pendingRegion, map.bounds.width > 1, map.bounds.height > 1 else {
            return
        }
        guard force || !regionApplied else {
            return
        }
        map.setRegion(region, animated: nextRegionAnimated)
        regionApplied = true
    }

    func mapViewDidFinishLoadingMap(_ mapView: MKMapView) {
        applyPendingRegionAfterLoad()
    }

    func mapViewDidFinishRenderingMap(_ mapView: MKMapView, fullyRendered: Bool) {
        applyPendingRegionAfterLoad()
    }

    private func applyPendingRegionAfterLoad() {
        guard !appliedAfterLoad, let region = pendingRegion else {
            return
        }
        map.setRegion(region, animated: false)
        regionApplied = true
        appliedAfterLoad = true
    }

    func mapView(_ mapView: MKMapView, viewFor annotation: MKAnnotation) -> MKAnnotationView? {
        guard let place = annotation as? PlaceAnnotation else {
            return nil
        }
        let identifier = "cheetah-place"
        let marker = mapView.dequeueReusableAnnotationView(withIdentifier: identifier)
            ?? MKAnnotationView(annotation: annotation, reuseIdentifier: identifier)
        marker.annotation = annotation
        marker.image = Self.markerImage(place)
        marker.canShowCallout = true
        marker.displayPriority = .required
        return marker
    }

    func mapView(_ mapView: MKMapView, didSelect annotation: MKAnnotation) {
        guard let place = annotation as? PlaceAnnotation, !place.placeID.isEmpty else {
            return
        }
        owner?.event("marker", place.placeID)
        mapView.deselectAnnotation(annotation, animated: false)
    }

    private static var markerImageCache: [String: UIImage] = [:]

    private static func markerImage(_ place: PlaceAnnotation) -> UIImage {
        let cacheKey = [
            place.placeID,
            place.kind,
            place.icon,
            place.iconColor,
            place.selected ? "selected" : "idle",
        ].joined(separator: "|")
        if let cached = markerImageCache[cacheKey] {
            return cached
        }
        let result = place.kind == "result"
        let size = result ? CGSize(width: 40, height: 40) : CGSize(width: 56, height: 56)
        let renderer = UIGraphicsImageRenderer(size: size)
        let image = renderer.image { context in
            let rect = CGRect(origin: .zero, size: size).insetBy(dx: 2, dy: 2)
            let circle = UIBezierPath(ovalIn: rect)
            context.cgContext.setShadow(
                offset: CGSize(width: 0, height: 2),
                blur: 4,
                color: UIColor.black.withAlphaComponent(0.3).cgColor
            )
            if result {
                (place.selected ? UIColor(hex: "#ef4444") : UIColor(hex: "#404040")).setFill()
                circle.fill()
                (place.selected ? UIColor.white : UIColor(hex: "#262626")).setStroke()
                circle.lineWidth = 3
                circle.stroke()
            } else {
                UIColor.white.setFill()
                circle.fill()
                UIColor(hex: "#262626").withAlphaComponent(0.5).setStroke()
                circle.lineWidth = 1
                circle.stroke()
            }

            let symbol = Self.symbols[place.icon] ?? "fork.knife"
            let configuration = UIImage.SymbolConfiguration(
                pointSize: result ? 14 : 20,
                weight: .regular
            )
            if let image = UIImage(systemName: symbol, withConfiguration: configuration)?
                .withTintColor(
                    result ? .white : UIColor(hex: place.iconColor),
                    renderingMode: .alwaysOriginal
                )
            {
                let side = result ? 16.0 : 22.0
                image.draw(
                    in: CGRect(
                        x: (size.width - side) / 2,
                        y: (size.height - side) / 2,
                        width: side,
                        height: side
                    )
                )
            }
        }
        markerImageCache[cacheKey] = image
        return image
    }

    private static let symbols: [String: String] = [
        "coffee": "cup.and.saucer.fill",
        "hamburger": "takeoutbag.and.cup.and.straw.fill",
        "pizza-slice": "triangle.fill",
        "fish": "fish.fill",
        "utensils": "fork.knife",
    ]
}

private final class SheetNativeModule: CheetahNativeModule {
    private weak var owner: NativeModuleHostView?
    private var controller: UIViewController?
    private let placeholder = UIView()
    private var sheetHeight: CGFloat = 230
    private var sheetRadius: CGFloat = 28
    private var searchBar: UIView?
    private var keyboardTop: CGFloat = 0
    private var keyboardObservers: [NSObjectProtocol] = []

    var nativeView: UIView {
        placeholder
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
    }

    func apply(props: [String: Any]) {
        var changed = false
        if let height = props["height"] as? NSNumber, CGFloat(height.doubleValue) != sheetHeight {
            sheetHeight = CGFloat(height.doubleValue)
            changed = true
        }
        if let radius = props["radius"] as? NSNumber, CGFloat(radius.doubleValue) != sheetRadius {
            sheetRadius = CGFloat(radius.doubleValue)
            changed = true
        }
        guard changed, let sheet = controller?.sheetPresentationController else {
            return
        }
        let identifier = UISheetPresentationController.Detent.Identifier("cheetah-search")
        let detent = UISheetPresentationController.Detent.custom(identifier: identifier) { _ in
            self.sheetHeight
        }
        sheet.animateChanges {
            sheet.detents = [detent]
            sheet.selectedDetentIdentifier = identifier
            sheet.largestUndimmedDetentIdentifier = identifier
        }
    }

    func layout() {
        updateKeyboardOffset()
    }

    private func installKeyboardObservers() {
        guard keyboardObservers.isEmpty else { return }
        let center = NotificationCenter.default
        keyboardObservers.append(center.addObserver(
            forName: UIResponder.keyboardWillChangeFrameNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            self?.readKeyboard(note)
        })
        keyboardObservers.append(center.addObserver(
            forName: UIResponder.keyboardWillHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.keyboardTop = 0
            self?.updateKeyboardOffset()
        })
    }

    private func readKeyboard(_ note: Notification) {
        guard
            let owner,
            let window = owner.window,
            let value = note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue
        else {
            return
        }
        let frame = window.convert(value.cgRectValue, from: nil)
        keyboardTop = frame.minY < window.bounds.maxY ? frame.minY : 0
        updateKeyboardOffset()
    }

    private func updateKeyboardOffset() {
        guard let owner, let window = owner.window else { return }
        if searchBar?.window == nil {
            searchBar = findSearchBar(owner)
        }
        guard let bar = searchBar, bar.window != nil else { return }
        if keyboardTop <= 0 {
            if abs(bar.transform.ty) > 0.5 {
                UIView.animate(withDuration: 0.22) { bar.transform = .identity }
            }
            return
        }
        guard let baseBottom = bar.superview?.convert(bar.frame, to: window).maxY else {
            return
        }
        let target = min(0, keyboardTop - 12 - baseBottom)
        if abs(bar.transform.ty - target) < 0.5 {
            return
        }
        UIView.animate(withDuration: 0.22) {
            bar.transform = CGAffineTransform(translationX: 0, y: target)
        }
    }

    private func findSearchBar(_ view: UIView) -> UIView? {
        if view.accessibilityIdentifier == "cheetah-search-bar" {
            return view
        }
        for child in view.subviews {
            if let match = findSearchBar(child) {
                return match
            }
        }
        return nil
    }

    func presentIfNeeded(owner: NativeModuleHostView) {
        guard controller == nil else {
            return
        }
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard let presenter = scene?.keyWindow?.rootViewController else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self, weak owner] in
                guard let self, let owner else {
                    return
                }
                self.presentIfNeeded(owner: owner)
            }
            return
        }

        let controller = UIViewController()
        controller.view.backgroundColor = UIColor(hex: "#171717")
        controller.isModalInPresentation = true
        owner.translatesAutoresizingMaskIntoConstraints = false
        controller.view.addSubview(owner)
        NSLayoutConstraint.activate([
            owner.leadingAnchor.constraint(equalTo: controller.view.leadingAnchor),
            owner.trailingAnchor.constraint(equalTo: controller.view.trailingAnchor),
            owner.topAnchor.constraint(equalTo: controller.view.topAnchor),
            owner.bottomAnchor.constraint(equalTo: controller.view.bottomAnchor),
        ])

        controller.modalPresentationStyle = .pageSheet
        if let sheet = controller.sheetPresentationController {
            let identifier = UISheetPresentationController.Detent.Identifier("cheetah-search")
            let detent = UISheetPresentationController.Detent.custom(identifier: identifier) { _ in
                self.sheetHeight
            }
            sheet.detents = [detent]
            sheet.selectedDetentIdentifier = identifier
            sheet.largestUndimmedDetentIdentifier = identifier
            sheet.prefersGrabberVisible = false
            sheet.preferredCornerRadius = sheetRadius
            sheet.prefersScrollingExpandsWhenScrolledToEdge = false
        }

        presenter.present(controller, animated: true)
        self.controller = controller
        installKeyboardObservers()
    }

    func dismiss() {
        controller?.dismiss(animated: false)
        controller = nil
    }

    func destroy() {
        let center = NotificationCenter.default
        for observer in keyboardObservers {
            center.removeObserver(observer)
        }
        keyboardObservers.removeAll()
        dismiss()
    }
}

private final class MenuNativeModule: CheetahNativeModule {
    let button = UIButton(type: .system)
    private weak var owner: NativeModuleHostView?
    private var items: [String] = []

    var nativeView: UIView {
        button
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        button.showsMenuAsPrimaryAction = true
        button.contentHorizontalAlignment = .center
    }

    func apply(props: [String: Any]) {
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            if let title = config["title"] as? String {
                button.setTitle(title, for: .normal)
            }
            items = config["items"] as? [String] ?? []
            let actions = items.map { item in
                UIAction(title: item) { [weak self] _ in
                    self?.owner?.event("select", item)
                }
            }
            button.menu = UIMenu(children: actions)
        }
        if let size = props["size"] as? NSNumber {
            button.titleLabel?.font = .systemFont(
                ofSize: CGFloat(size.doubleValue),
                weight: (props["bold"] as? NSNumber)?.boolValue == true ? .bold : .regular
            )
        }
        if let color = props["color"] as? String {
            button.setTitleColor(UIColor(hex: color), for: .normal)
        }
    }
}

private final class IconNativeModule: CheetahNativeModule {
    private let imageView = UIImageView()
    private var iconName = "circle.fill"

    var nativeView: UIView {
        imageView
    }

    init() {
        imageView.contentMode = .scaleAspectFit
        imageView.clipsToBounds = true
        imageView.tintColor = .white
    }

    func apply(props: [String: Any]) {
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            iconName = config["name"] as? String ?? iconName
            if let color = config["color"] as? String {
                imageView.tintColor = UIColor(hex: color)
            }
        }
        if let color = props["color"] as? String {
            imageView.tintColor = UIColor(hex: color)
        }
        let symbol = Self.symbols[iconName] ?? iconName
        let config = UIImage.SymbolConfiguration(
            pointSize: CGFloat((props["size"] as? NSNumber)?.doubleValue ?? 16),
            weight: .regular
        )
        imageView.image = UIImage(systemName: symbol, withConfiguration: config)?
            .withRenderingMode(.alwaysTemplate)
        imageView.accessibilityLabel = iconName
    }

    private static let symbols: [String: String] = [
        "utensils": "fork.knife",
        "arrow-up": "arrow.up",
        "xmark": "xmark",
        "chevron-right": "chevron.right",
        "chevron-up": "chevron.up",
        "bookmark": "bookmark",
        "share": "square.and.arrow.up",
        "plus": "plus",
        "coffee": "cup.and.saucer.fill",
        "hamburger": "takeoutbag.and.cup.and.straw.fill",
        "fish": "fish.fill",
        "pizza-slice": "triangle.fill",
        "car": "car.fill",
        "google": "g.circle.fill",
        "magnifying-glass": "magnifyingglass",
        "circle-xmark": "xmark.circle.fill",
        "star": "star.fill",
    ]
}

private final class PhotoPagerNativeModule: CheetahNativeModule {
    private let pager = PhotoPagerView()

    var nativeView: UIView {
        pager
    }

    func apply(props: [String: Any]) {
        guard let raw = props["nativeProps"] as? String,
              let data = raw.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return
        }
        pager.configure(
            src: config["src"] as? String ?? "asset://second-cup-card-full.jpeg",
            count: (config["pages"] as? NSNumber)?.intValue ?? 5
        )
    }
}

private final class GradientNativeModule: CheetahNativeModule {
    private let gradientView = GradientView()

    var nativeView: UIView {
        gradientView
    }

    func apply(props: [String: Any]) {
        guard let raw = props["nativeProps"] as? String,
              let data = raw.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return
        }
        let colors = (config["colors"] as? [String] ?? []).map { UIColor(hex: $0).cgColor }
        let locations = (config["locations"] as? [NSNumber] ?? []).map {
            NSNumber(value: $0.doubleValue)
        }
        gradientView.apply(
            colors: colors,
            locations: locations,
            start: CGPoint(
                x: (config["startX"] as? NSNumber)?.doubleValue ?? 0,
                y: (config["startY"] as? NSNumber)?.doubleValue ?? 0
            ),
            end: CGPoint(
                x: (config["endX"] as? NSNumber)?.doubleValue ?? 0,
                y: (config["endY"] as? NSNumber)?.doubleValue ?? 1
            )
        )
    }
}

private final class GradientView: UIView {
    private let gradient = CAGradientLayer()

    override init(frame: CGRect) {
        super.init(frame: frame)
        layer.addSublayer(gradient)
        isUserInteractionEnabled = false
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        gradient.frame = bounds
    }

    func apply(colors: [CGColor], locations: [NSNumber], start: CGPoint, end: CGPoint) {
        gradient.colors = colors
        gradient.locations = locations
        gradient.startPoint = start
        gradient.endPoint = end
        setNeedsLayout()
    }
}

private final class SearchNativeModule: NSObject, CheetahNativeModule, UITextFieldDelegate {
    let field = UISearchTextField()
    weak var owner: NativeModuleHostView?
    private var updating = false

    var nativeView: UIView {
        field
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        super.init()

        field.delegate = self
        field.borderStyle = .none
        field.textColor = .white
        field.tintColor = .white
        field.backgroundColor = .clear
        field.autocapitalizationType = .sentences
        field.returnKeyType = .send
        field.addTarget(self, action: #selector(textChanged(_:)), for: .editingChanged)
    }

    func apply(props: [String: Any]) {
        if let placeholder = props["placeholder"] as? String {
            field.attributedPlaceholder = NSAttributedString(
                string: placeholder,
                attributes: [.foregroundColor: UIColor(hex: "#a3a3a3")]
            )
        }
        if let value = props["value"] as? String, field.text != value {
            updating = true
            field.text = value
            updating = false
        }
        if let size = props["size"] as? NSNumber {
            field.font = .systemFont(ofSize: CGFloat(size.doubleValue))
        }
        if let color = props["color"] as? String {
            field.textColor = UIColor(hex: color)
        }
    }

    @objc private func textChanged(_ field: UITextField) {
        guard !updating else {
            return
        }
        owner?.event("change", field.text ?? "")
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        owner?.event("submit", "")
        return true
    }
}
