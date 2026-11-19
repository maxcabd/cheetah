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

/// A native module shown in its own presented screen (the sheet, pages):
/// its host view moves into that screen instead of its parent.
protocol CheetahPresenting: AnyObject {
    func presentIfNeeded(owner: NativeModuleHostView)
}

/// The view with accessibility identifier (`test_id`) `identifier` under
/// `view`, if any.
func findView(in view: UIView, identifier: String) -> UIView? {
    if view.accessibilityIdentifier == identifier {
        return view
    }
    for child in view.subviews {
        if let match = findView(in: child, identifier: identifier) {
            return match
        }
    }
    return nil
}

/// The screen on top: the root, or whatever it presents, all the way up.
func topmostController(in window: UIWindow?) -> UIViewController? {
    var top = window?.rootViewController
    while let presented = top?.presentedViewController, !presented.isBeingDismissed {
        top = presented
    }
    return top
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
        if let presenting = module as? CheetahPresenting {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                presenting.presentIfNeeded(owner: self)
            }
        }
        setNeedsLayout()
    }

    private func materialize(name: String, props: [String: Any]) {
        let created = NativeModuleRegistry.make(name: name, owner: self)
        module = created
        if !(created is CheetahPresenting), let native = created?.nativeView {
            addSubview(native)
        }
        created?.apply(props: props)
        if !hostedChildren.isEmpty {
            (created as? CheetahChildHosting)?.setChildren(hostedChildren)
        }
        if let presenting = created as? CheetahPresenting {
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                presenting.presentIfNeeded(owner: self)
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
        case "page":
            return PageNativeModule(owner: owner)
        case "menu":
            return MenuNativeModule(owner: owner)
        case "icon":
            return IconNativeModule()
        case "gradient":
            return GradientNativeModule()
        case "blur":
            return BlurNativeModule()
        case "shape":
            return ShapeNativeModule()
        case "photo":
            return PhotoPagerNativeModule(owner: owner)
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
    private var lastRecenter: Int?
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
        //   recenter: a counter; changing it re-applies the region
        //   pointsOfInterest, buildings: false by default
        var config: [String: Any] = [:]
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            config = parsed
        }
        // `recenter`: a counter; a new value moves the map back to its region
        // even if the region didn't change (the user panned away by hand).
        let recenter = (config["recenter"] as? NSNumber)?.intValue ?? 0
        if let lastRecenter, recenter != lastRecenter, lastRegionKey != nil {
            lastRegionKey = "recenter"
        }
        lastRecenter = recenter

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

/// `native "page"`: a full-screen page over the app, its content the
/// element's children (a router page). native_props:
///   transition: "zoom" grows it out of the element whose test_id is
///     `source` (iOS 18+; drag down shrinks it back), else it slides up.
///   source: that element's test_id.
///   background: color behind the content (default "#0b0b0f").
/// Closed by the user (drag down), it reports `dismiss`; removed by the app
/// (router back), it closes itself, zooming back into its source.
private final class PageNativeModule: NSObject, CheetahNativeModule, CheetahPresenting, UIAdaptivePresentationControllerDelegate {
    private weak var owner: NativeModuleHostView?
    private var controller: PageController?
    private let placeholder = UIView()
    private var style: [String: Any] = [:]

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
        controller?.view.backgroundColor = background
    }

    private var background: UIColor {
        UIColor(hex: style["background"] as? String ?? "#0b0b0f")
    }

    func presentIfNeeded(owner: NativeModuleHostView) {
        // Closed by iOS without us noticing: present it again.
        if let shown = controller, shown.presentingViewController == nil, !shown.isBeingPresented {
            controller = nil
        }
        guard controller == nil else {
            return
        }
        let window = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }?
            .keyWindow
        // Present over whatever is on top (the sheet), once it is settled.
        guard
            let presenter = topmostController(in: window),
            presenter.viewIfLoaded?.window != nil,
            !presenter.isBeingPresented,
            !presenter.isBeingDismissed
        else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self, weak owner] in
                guard let self, let owner else { return }
                self.presentIfNeeded(owner: owner)
            }
            return
        }

        let controller = PageController()
        controller.view.backgroundColor = background
        owner.translatesAutoresizingMaskIntoConstraints = false
        controller.view.addSubview(owner)
        NSLayoutConstraint.activate([
            owner.leadingAnchor.constraint(equalTo: controller.view.leadingAnchor),
            owner.trailingAnchor.constraint(equalTo: controller.view.trailingAnchor),
            owner.topAnchor.constraint(equalTo: controller.view.topAnchor),
            owner.bottomAnchor.constraint(equalTo: controller.view.bottomAnchor),
        ])
        controller.modalPresentationStyle = .fullScreen
        controller.modalTransitionStyle = .coverVertical

        let zoom = style["transition"] as? String == "zoom"
        if zoom, #available(iOS 18.0, *), let source = style["source"] as? String {
            // Looked up each time: the source may have moved or been redrawn
            // by the time the page closes.
            controller.preferredTransition = .zoom { [weak window] _ in
                guard let window else { return nil }
                return findView(in: window, identifier: source)
            }
        }
        controller.onDismissedByUser = { [weak self] in
            self?.owner?.event("dismiss", "")
        }
        // A drag down (with the zoom transition too) ends here.
        controller.presentationController?.delegate = self
        presenter.present(controller, animated: true)
        self.controller = controller
    }

    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        controller?.onDismissedByUser?()
        controller?.onDismissedByUser = nil
    }

    func destroy() {
        guard let controller else { return }
        self.controller = nil
        controller.onDismissedByUser = nil
        if controller.presentingViewController != nil, !controller.isBeingDismissed {
            controller.dismiss(animated: true)
        }
    }
}

/// A page's screen; tells when the user closed it (not the app).
private final class PageController: UIViewController {
    var onDismissedByUser: (() -> Void)?

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        if isBeingDismissed {
            onDismissedByUser?()
            onDismissedByUser = nil
        }
    }
}

/// The native bottom sheet (`sheet` in .view files), configured by
/// `native_props`:
///   detents: [{"id": "rest", "height": 120}, {"id": "expanded", ...}]: the
///     heights it rests at. iOS drags between them natively and reports where
///     the user left it as a `select` event (payload: the id). Without
///     detents it has one, the element's `height`.
///   selected: the detent to show; changing it animates natively.
///   keyboardView: test_id of a child kept above the keyboard, moved with the
///     keyboard's own animation; keyboardGap: points above it (default 12).
///   background: color behind the content (default "#171717")
///   grabber: system grabber (default false: views draw their own)
///   cornerRadius: (default: the element's `radius`)
/// A drag below the lowest detent reports `dismiss`: the sheet can't close,
/// so the app decides what it means.
private final class SheetNativeModule: NSObject, CheetahNativeModule, CheetahPresenting, UISheetPresentationControllerDelegate {
    private weak var owner: NativeModuleHostView?
    private var style: [String: Any] = [:]
    private var detentIDs: [String] = []
    private var detentHeights: [String: CGFloat] = [:]
    private var selectedDetent: String?
    private var appliedDetentKey: String?
    private var keyboardDuration: TimeInterval = 0.25
    private var keyboardCurve: UInt = 7
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
        if let height = props["height"] as? NSNumber {
            sheetHeight = CGFloat(height.doubleValue)
        }
        readDetents()
        if let controller {
            applyStyle(controller)
            if let sheet = controller.sheetPresentationController {
                applyDetents(sheet, animated: true)
            }
        }
    }

    /// Detents from `native_props`, or one at the element's height.
    private func readDetents() {
        if let list = style["detents"] as? [[String: Any]], !list.isEmpty {
            detentIDs = list.compactMap { $0["id"] as? String }
            for item in list {
                if let id = item["id"] as? String, let height = item["height"] as? NSNumber {
                    detentHeights[id] = CGFloat(height.doubleValue)
                }
            }
            selectedDetent = style["selected"] as? String ?? detentIDs.last
        } else {
            detentIDs = ["cheetah-search"]
            detentHeights["cheetah-search"] = sheetHeight
            selectedDetent = "cheetah-search"
        }
    }

    private func applyDetents(_ sheet: UISheetPresentationController, animated: Bool) {
        let key = detentIDs.map { "\($0)=\(detentHeights[$0] ?? 0)" }.joined(separator: ",")
            + "|" + (selectedDetent ?? "")
        guard key != appliedDetentKey else {
            return
        }
        let idsChanged = sheet.detents.map(\.identifier.rawValue) != detentIDs
        appliedDetentKey = key
        let changes = {
            if idsChanged {
                sheet.detents = self.detentIDs.map { id in
                    UISheetPresentationController.Detent.custom(
                        identifier: UISheetPresentationController.Detent.Identifier(id)
                    ) { [weak self] context in
                        min(self?.detentHeights[id] ?? 230, context.maximumDetentValue)
                    }
                }
            } else {
                sheet.invalidateDetents()
            }
            // Never dim the map behind the sheet.
            if let largest = self.detentIDs.max(by: {
                (self.detentHeights[$0] ?? 0) < (self.detentHeights[$1] ?? 0)
            }) {
                sheet.largestUndimmedDetentIdentifier = UISheetPresentationController.Detent.Identifier(largest)
            }
            if let selected = self.selectedDetent, sheet.selectedDetentIdentifier?.rawValue != selected {
                sheet.selectedDetentIdentifier = UISheetPresentationController.Detent.Identifier(selected)
            }
        }
        if animated {
            sheet.animateChanges(changes)
        } else {
            changes()
        }
    }

    /// The user dragged the sheet to another detent.
    func sheetPresentationControllerDidChangeSelectedDetentIdentifier(
        _ sheetPresentationController: UISheetPresentationController
    ) {
        guard let id = sheetPresentationController.selectedDetentIdentifier?.rawValue else {
            return
        }
        selectedDetent = id
        appliedDetentKey = nil
        owner?.event("select", id)
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
        // iOS holds a sheet at its largest detent while the keyboard is up
        // and ignores a smaller `selected` meanwhile: apply it once the
        // keyboard is gone.
        keyboardObservers.append(center.addObserver(
            forName: UIResponder.keyboardDidHideNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            guard let self, let sheet = self.controller?.sheetPresentationController else { return }
            self.appliedDetentKey = nil
            self.applyDetents(sheet, animated: true)
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
        // Move with the keyboard's own timing.
        if let duration = note.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? NSNumber {
            keyboardDuration = duration.doubleValue
        }
        if let curve = note.userInfo?[UIResponder.keyboardAnimationCurveUserInfoKey] as? NSNumber {
            keyboardCurve = curve.uintValue
        }
        updateKeyboardOffset()
    }

    private func updateKeyboardOffset() {
        guard let owner, let window = owner.window,
              let identifier = style["keyboardView"] as? String
        else {
            return
        }
        if searchBar?.window == nil || searchBar?.accessibilityIdentifier != identifier {
            searchBar = findView(in: owner, identifier: identifier)
        }
        guard let bar = searchBar, bar.window != nil else { return }
        let options = UIView.AnimationOptions(rawValue: keyboardCurve << 16)
            .union([.beginFromCurrentState, .allowUserInteraction])
        if keyboardTop <= 0 {
            if abs(bar.transform.ty) > 0.5 {
                UIView.animate(withDuration: keyboardDuration, delay: 0, options: options) {
                    bar.transform = .identity
                }
            }
            return
        }
        // Where the bar sits without the lift.
        guard let laidOut = bar.superview?.convert(bar.frame, to: window) else {
            return
        }
        let baseBottom = laidOut.maxY - bar.transform.ty
        let gap = CGFloat((style["keyboardGap"] as? NSNumber)?.doubleValue ?? 12)
        let target = min(0, keyboardTop - gap - baseBottom)
        if abs(bar.transform.ty - target) < 0.5 {
            return
        }
        UIView.animate(withDuration: keyboardDuration, delay: 0, options: options) {
            bar.transform = CGAffineTransform(translationX: 0, y: target)
        }
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
        // The sheet's own view isn't on screen until it is presented, so
        // present from the active window's root screen, once that screen is
        // on screen and free: after a dev reconnect or link the previous
        // sheet may still be dismissing, and presenting then fails, leaving
        // no sheet at all.
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard
            let presenter = scene?.keyWindow?.rootViewController,
            presenter.viewIfLoaded?.window != nil,
            presenter.presentedViewController == nil,
            !presenter.isBeingDismissed
        else {
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
            sheet.prefersScrollingExpandsWhenScrolledToEdge = false
            appliedDetentKey = nil
            applyDetents(sheet, animated: false)
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

/// `native "photo"`: a paged photo. Once the first image loads it reports
/// a `load` event, `{"index": 0, "color": "#rrggbb"}`: the image's dominant
/// color, for views that match it (a card shrunk to its color).
private final class PhotoPagerNativeModule: CheetahNativeModule {
    private let pager = PhotoPagerView()

    var nativeView: UIView {
        pager
    }

    init(owner: NativeModuleHostView) {
        pager.onDominantColor = { [weak owner] color in
            let payload = ["index": 0, "color": color] as [String: Any]
            guard
                let data = try? JSONSerialization.data(withJSONObject: payload),
                let text = String(data: data, encoding: .utf8)
            else {
                return
            }
            owner?.event("load", text)
        }
    }

    func apply(props: [String: Any]) {
        guard let raw = props["nativeProps"] as? String,
              let data = raw.data(using: .utf8),
              let config = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return
        }
        // `photos`: a page per photo (asset:// or http(s)://); else one
        // `src` repeated over `pages`.
        if let photos = config["photos"] as? [String], !photos.isEmpty {
            pager.configure(photos: photos)
        } else {
            pager.configure(
                src: config["src"] as? String ?? "asset://second-cup-card-full.jpeg",
                count: (config["pages"] as? NSNumber)?.intValue ?? 5
            )
        }
    }
}

/// `native "blur"`: blurs what is behind it. native_props:
///   style: "dark" (default), "light", "regular", or a material: "thin",
///     "thick", "ultraThin", "chrome" (dark variants).
///   fade: "top" fades the blur in from clear at the top to full at the
///     bottom (a soft edge, like a photo blurring into a page); "bottom" the
///     reverse; absent: even blur.
private final class BlurNativeModule: CheetahNativeModule {
    private let blurView = BlurView()

    var nativeView: UIView {
        blurView
    }

    func apply(props: [String: Any]) {
        var config: [String: Any] = [:]
        if let raw = props["nativeProps"] as? String,
           let data = raw.data(using: .utf8),
           let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        {
            config = parsed
        }
        blurView.configure(style: config["style"] as? String ?? "dark", fade: config["fade"] as? String)
    }
}

private final class BlurView: UIView {
    private let effectView = UIVisualEffectView(effect: nil)
    private let fadeMaskView = UIView()
    private let maskGradient = CAGradientLayer()
    private var fade: String?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isUserInteractionEnabled = false
        addSubview(effectView)
        fadeMaskView.layer.addSublayer(maskGradient)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(style: String, fade: String?) {
        let blurStyle: UIBlurEffect.Style
        switch style {
        case "light": blurStyle = .light
        case "regular": blurStyle = .regular
        case "thin": blurStyle = .systemThinMaterialDark
        case "thick": blurStyle = .systemThickMaterialDark
        case "ultraThin": blurStyle = .systemUltraThinMaterialDark
        case "chrome": blurStyle = .systemChromeMaterialDark
        default: blurStyle = .dark
        }
        effectView.effect = UIBlurEffect(style: blurStyle)
        self.fade = fade
        // An alpha mask: clear lets the photo through, opaque is full blur.
        maskGradient.colors = [UIColor.clear.cgColor, UIColor.black.cgColor]
        maskGradient.startPoint = CGPoint(x: 0.5, y: fade == "bottom" ? 1 : 0)
        maskGradient.endPoint = CGPoint(x: 0.5, y: fade == "bottom" ? 0 : 1)
        effectView.mask = fade == nil ? nil : fadeMaskView
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        effectView.frame = bounds
        fadeMaskView.frame = bounds
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        maskGradient.frame = bounds
        CATransaction.commit()
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
