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

/// A native module that shows its element's children itself, e.g. the map
/// placing each child at its `map_latitude`/`map_longitude`. The children are
/// ordinary views built from `.view` templates; only placement is native.
protocol CheetahChildHosting: AnyObject {
    func setChildren(_ children: [(view: UIView, props: [String: Any])])
}

/// Container for a semantic native module. The Rust layout owns this box;
/// the module owns the platform control and its internal behavior.
final class NativeModuleHostView: UIView {
    private(set) var module: CheetahNativeModule?
    var onEvent: ((String, String) -> Void)?
    private var pendingName: String?
    private var pendingProps: [String: Any]?
    private var mapScheduled = false
    private var hostedChildren: [(view: UIView, props: [String: Any])] = []

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
            backgroundColor = (props["bg"] as? String).map { UIColor(hex: $0) }
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
        if !hostedChildren.isEmpty {
            (created as? CheetahChildHosting)?.setChildren(hostedChildren)
        }
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

    /// The element's children, for modules that place them (CheetahChildHosting).
    /// Kept until the module exists: the map materializes only once sized.
    func setChildren(_ children: [(view: UIView, props: [String: Any])]) {
        hostedChildren = children
        (module as? CheetahChildHosting)?.setChildren(children)
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
        case "shape":
            return ShapeNativeModule()
        case "photo":
            return PhotoPagerNativeModule()
        default:
            return ModuleRegistry.views[name].map { $0.init(owner: owner) }
        }
    }
}

/// A child view of the map element, pinned at its coordinate.
private final class ViewAnnotation: NSObject, MKAnnotation {
    @objc dynamic var coordinate: CLLocationCoordinate2D
    let content: UIView
    var order: Int

    init(content: UIView, coordinate: CLLocationCoordinate2D, order: Int) {
        self.content = content
        self.coordinate = coordinate
        self.order = order
    }
}

/// Hosts a map child's view, centered on its coordinate and sized by the
/// Rust layout. Taps go to the child (`pressable`), not to MapKit.
final class ViewAnnotationView: MKAnnotationView {
    func host(_ content: UIView) {
        if content.superview !== self {
            subviews.forEach { $0.removeFromSuperview() }
            addSubview(content)
        }
        fit(content.bounds.size)
    }

    /// The Rust layout sized the child: match it and keep it centered.
    func fit(_ size: CGSize) {
        bounds = CGRect(origin: .zero, size: size)
        for content in subviews {
            content.bounds.size = size
            content.center = CGPoint(x: size.width / 2, y: size.height / 2)
        }
    }

    /// A child's `map_latitude`/`map_longitude` changed.
    func move(to props: [String: Any]) {
        guard
            let annotation = annotation as? ViewAnnotation,
            let coordinate = MapNativeModule.coordinate(props)
        else {
            return
        }
        annotation.coordinate = coordinate
    }
}

private final class MapNativeModule: NSObject, CheetahNativeModule, CheetahChildHosting, MKMapViewDelegate {
    let map: MKMapView
    private weak var owner: NativeModuleHostView?
    private var pendingRegion: MKCoordinateRegion?
    private var regionApplied = false
    private var appliedAfterLoad = false
    private var lastRegionKey: String?
    private var lastDark: Bool?
    private var lastShowUser: Bool?
    private var lastMapType: String?
    private var childAnnotations: [ObjectIdentifier: ViewAnnotation] = [:]
    private var nextRegionAnimated = false

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

        if let background = props["bg"] as? String {
            map.backgroundColor = UIColor(hex: background)
        }

        // The map's look, from `native_props`:
        //   mapType: "muted" (default), "standard", "satellite", "hybrid"
        //   pointsOfInterest, buildings: false by default
        var config: [String: Any] = [:]
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            config = parsed
        }
        let type = config["mapType"] as? String ?? "muted"
        if type != lastMapType {
            lastMapType = type
            switch type {
            case "standard": map.mapType = .standard
            case "satellite": map.mapType = .satellite
            case "hybrid": map.mapType = .hybrid
            default: map.mapType = .mutedStandard
            }
        }
        let pointsOfInterest = (config["pointsOfInterest"] as? Bool) ?? false
        map.pointOfInterestFilter = pointsOfInterest ? .includingAll : .excludingAll
        map.showsBuildings = (config["buildings"] as? Bool) ?? false

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

    static func coordinate(_ props: [String: Any]) -> CLLocationCoordinate2D? {
        guard
            let latitude = props["mapLatitude"] as? NSNumber,
            let longitude = props["mapLongitude"] as? NSNumber
        else {
            return nil
        }
        return CLLocationCoordinate2D(latitude: latitude.doubleValue, longitude: longitude.doubleValue)
    }

    /// Children with a coordinate become pins; later children draw on top.
    func setChildren(_ children: [(view: UIView, props: [String: Any])]) {
        var kept: [ObjectIdentifier: ViewAnnotation] = [:]
        for (order, child) in children.enumerated() {
            guard let coordinate = Self.coordinate(child.props) else {
                continue
            }
            let key = ObjectIdentifier(child.view)
            if let existing = childAnnotations.removeValue(forKey: key) {
                existing.coordinate = coordinate
                existing.order = order
                kept[key] = existing
                if let view = map.view(for: existing) {
                    view.zPriority = MKAnnotationViewZPriority(rawValue: Float(order))
                    (view as? ViewAnnotationView)?.host(child.view)
                }
            } else {
                let annotation = ViewAnnotation(content: child.view, coordinate: coordinate, order: order)
                kept[key] = annotation
                map.addAnnotation(annotation)
            }
        }
        map.removeAnnotations(Array(childAnnotations.values))
        childAnnotations = kept
    }

    func mapView(_ mapView: MKMapView, viewFor annotation: MKAnnotation) -> MKAnnotationView? {
        if let child = annotation as? ViewAnnotation {
            // Not reused: each pin owns its child view.
            let view = ViewAnnotationView(annotation: child, reuseIdentifier: nil)
            view.host(child.content)
            view.displayPriority = .required
            view.zPriority = MKAnnotationViewZPriority(rawValue: Float(child.order))
            view.canShowCallout = false
            return view
        }
        return nil
    }

    func mapView(_ mapView: MKMapView, didSelect annotation: MKAnnotation) {
        if annotation is ViewAnnotation {
            // The child view handles its own taps.
            mapView.deselectAnnotation(annotation, animated: false)
            return
        }
    }
}

/// The native bottom sheet (`sheet` in .view files). A drag down reports a
/// `dismiss` event: the sheet can't close, so the app decides what it means.
/// Its look comes from `native_props`:
///   background: color behind the content (default "#171717")
///   grabber: system grabber (default false: views draw their own)
///   cornerRadius: (default: the element's `radius`)
private final class SheetNativeModule: NSObject, CheetahNativeModule, UIAdaptivePresentationControllerDelegate {
    private weak var owner: NativeModuleHostView?
    private var style: [String: Any] = [:]
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
        super.init()
    }

    func apply(props: [String: Any]) {
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            style = parsed
        }
        if let radius = props["radius"] as? NSNumber {
            sheetRadius = CGFloat(radius.doubleValue)
        }
        if let controller {
            applyStyle(controller)
        }
        var changed = false
        if let height = props["height"] as? NSNumber, CGFloat(height.doubleValue) != sheetHeight {
            sheetHeight = CGFloat(height.doubleValue)
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

    private func applyStyle(_ controller: UIViewController) {
        controller.view.backgroundColor = UIColor(hex: style["background"] as? String ?? "#171717")
        if let sheet = controller.sheetPresentationController {
            sheet.prefersGrabberVisible = (style["grabber"] as? Bool) ?? false
            sheet.preferredCornerRadius = (style["cornerRadius"] as? NSNumber)
                .map { CGFloat($0.doubleValue) } ?? sheetRadius
        }
    }

    func presentationControllerDidAttemptToDismiss(_ presentationController: UIPresentationController) {
        owner?.event("dismiss", "")
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
            sheet.prefersScrollingExpandsWhenScrolledToEdge = false
        }
        applyStyle(controller)
        // The sheet can't be dismissed, so a drag down only "attempts" it.
        controller.presentationController?.delegate = self

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

/// `native "shape"`: an SVG path scaled from `viewBox` to the element's size,
/// filled with a color or a gradient and optionally stroked. native_props:
///   path: SVG path data (M L H V C S Q T Z, absolute or relative)
///   viewBox: [width, height] the path is drawn in
///   fill: "#rrggbb[aa]", or gradient: {colors, locations, startX, startY,
///     endX, endY} (unit coordinates, like `native "gradient"`)
///   stroke: "#rrggbb[aa]", strokeWidth: points
private final class ShapeNativeModule: CheetahNativeModule {
    private let shapeView = ShapeView()

    var nativeView: UIView {
        shapeView
    }

    func apply(props: [String: Any]) {
        guard let raw = props["nativeProps"] as? String,
              let data = raw.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return
        }
        shapeView.configure(config)
    }
}

private func shapeNumber(_ value: Any?, _ fallback: Double) -> CGFloat {
    CGFloat((value as? NSNumber)?.doubleValue ?? fallback)
}

private final class ShapeView: UIView {
    private let fillLayer = CAGradientLayer()
    private let fillMask = CAShapeLayer()
    private let strokeLayer = CAShapeLayer()
    private var path: CGPath = CGMutablePath()
    private var viewBox = CGSize(width: 1, height: 1)

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isUserInteractionEnabled = false
        fillLayer.mask = fillMask
        layer.addSublayer(fillLayer)
        strokeLayer.fillColor = nil
        strokeLayer.lineJoin = .round
        layer.addSublayer(strokeLayer)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(_ config: [String: Any]) {
        path = SVGPath.parse(config["path"] as? String ?? "")
        if let box = config["viewBox"] as? [NSNumber], box.count == 2 {
            viewBox = CGSize(width: box[0].doubleValue, height: box[1].doubleValue)
        } else {
            let bounds = path.boundingBoxOfPath
            viewBox = CGSize(width: bounds.maxX, height: bounds.maxY)
        }
        if let gradient = config["gradient"] as? [String: Any] {
            fillLayer.colors = (gradient["colors"] as? [String] ?? []).map { UIColor(hex: $0).cgColor }
            fillLayer.locations = gradient["locations"] as? [NSNumber]
            fillLayer.startPoint = CGPoint(
                x: shapeNumber(gradient["startX"], 0),
                y: shapeNumber(gradient["startY"], 0)
            )
            fillLayer.endPoint = CGPoint(
                x: shapeNumber(gradient["endX"], 0),
                y: shapeNumber(gradient["endY"], 1)
            )
        } else {
            let color = (config["fill"] as? String).map { UIColor(hex: $0).cgColor } ?? UIColor.clear.cgColor
            fillLayer.colors = [color, color]
            fillLayer.locations = nil
        }
        strokeLayer.strokeColor = (config["stroke"] as? String).map { UIColor(hex: $0).cgColor }
        strokeLayer.lineWidth = shapeNumber(config["strokeWidth"], 0)
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        var transform = CGAffineTransform(
            scaleX: bounds.width / max(viewBox.width, 0.001),
            y: bounds.height / max(viewBox.height, 0.001)
        )
        let scaled = path.copy(using: &transform)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fillLayer.frame = bounds
        fillMask.frame = bounds
        fillMask.path = scaled
        strokeLayer.frame = bounds
        strokeLayer.path = scaled
        CATransaction.commit()
    }
}

/// SVG path data -> CGPath: M L H V C S Q T Z, absolute and relative.
/// Parsing stops at anything else (e.g. arcs).
private enum SVGPath {
    private enum Token {
        case command(Character)
        case number(CGFloat)
    }

    private static func tokenize(_ data: String) -> [Token] {
        var tokens: [Token] = []
        var number = ""
        func flush() {
            if let value = Double(number) {
                tokens.append(.number(CGFloat(value)))
            }
            number = ""
        }
        for character in data {
            if character.isLetter, character != "e", character != "E" {
                flush()
                tokens.append(.command(character))
            } else if character == "-" || character == "+" {
                if number.last == "e" || number.last == "E" {
                    number.append(character)
                } else {
                    flush()
                    number.append(character)
                }
            } else if character == "." {
                if number.contains(".") {
                    flush()
                }
                number.append(character)
            } else if character.isNumber || character == "e" || character == "E" {
                number.append(character)
            } else {
                flush()
            }
        }
        flush()
        return tokens
    }

    static func parse(_ data: String) -> CGPath {
        let path = CGMutablePath()
        let tokens = tokenize(data)
        var index = 0
        var command: Character = "M"
        var current = CGPoint.zero
        var start = CGPoint.zero
        var lastCubic: CGPoint?
        var lastQuad: CGPoint?

        func number() -> CGFloat? {
            guard index < tokens.count, case .number(let value) = tokens[index] else {
                return nil
            }
            index += 1
            return value
        }

        while index < tokens.count {
            if case .command(let next) = tokens[index] {
                command = next
                index += 1
            }
            let relative = command.isLowercase
            let origin = relative ? current : .zero
            func point() -> CGPoint? {
                guard let x = number(), let y = number() else { return nil }
                return CGPoint(x: origin.x + x, y: origin.y + y)
            }
            var cubic: CGPoint?
            var quad: CGPoint?

            switch Character(command.uppercased()) {
            case "M":
                guard let target = point() else { return path }
                path.move(to: target)
                current = target
                start = target
                // Further pairs after a moveto are linetos.
                command = relative ? "l" : "L"
            case "L":
                guard let target = point() else { return path }
                path.addLine(to: target)
                current = target
            case "H":
                guard let x = number() else { return path }
                current = CGPoint(x: origin.x + x, y: current.y)
                path.addLine(to: current)
            case "V":
                guard let y = number() else { return path }
                current = CGPoint(x: current.x, y: origin.y + y)
                path.addLine(to: current)
            case "C":
                guard let control1 = point(), let control2 = point(), let target = point() else {
                    return path
                }
                path.addCurve(to: target, control1: control1, control2: control2)
                cubic = control2
                current = target
            case "S":
                let control1 = lastCubic.map { CGPoint(x: 2 * current.x - $0.x, y: 2 * current.y - $0.y) } ?? current
                guard let control2 = point(), let target = point() else { return path }
                path.addCurve(to: target, control1: control1, control2: control2)
                cubic = control2
                current = target
            case "Q":
                guard let control = point(), let target = point() else { return path }
                path.addQuadCurve(to: target, control: control)
                quad = control
                current = target
            case "T":
                let control = lastQuad.map { CGPoint(x: 2 * current.x - $0.x, y: 2 * current.y - $0.y) } ?? current
                guard let target = point() else { return path }
                path.addQuadCurve(to: target, control: control)
                quad = control
                current = target
            case "Z":
                path.closeSubpath()
                current = start
                // Z takes no numbers: the next token must be a command.
                if index < tokens.count, case .number = tokens[index] {
                    return path
                }
            default:
                return path
            }
            lastCubic = cubic
            lastQuad = quad
        }
        return path
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
