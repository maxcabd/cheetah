import MapKit
import UIKit

protocol CheetahNativeModule: AnyObject {
    var nativeView: UIView { get }
    func apply(props: [String: Any])
    func destroy()
}

extension CheetahNativeModule {
    func destroy() {}
}

/// Container for a semantic native module. The Rust layout owns this box;
/// the module owns the platform control and its internal behavior.
final class NativeModuleHostView: UIView {
    private(set) var module: CheetahNativeModule?
    var onEvent: ((String, String) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(name: String, props: [String: Any]) {
        if module == nil {
            module = NativeModuleRegistry.make(name: name, owner: self)
            if name != "sheet", let native = module?.nativeView {
                addSubview(native)
            }
        }
        module?.apply(props: props)
        if let sheet = module as? SheetNativeModule {
            sheet.presentIfNeeded(owner: self)
        }
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        module?.nativeView.frame = bounds
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
        default:
            return nil
        }
    }
}

private final class MapNativeModule: NSObject, CheetahNativeModule, MKMapViewDelegate {
    let map: MKMapView
    private weak var owner: NativeModuleHostView?

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
        map.showsCompass = false
        map.showsScale = false
        map.isRotateEnabled = false
        map.isPitchEnabled = false
        map.delegate = self
    }

    func apply(props: [String: Any]) {
        if let dark = props["mapDark"] as? NSNumber {
            map.overrideUserInterfaceStyle = dark.boolValue ? .dark : .light
        }

        if let latitude = props["mapLatitude"] as? NSNumber,
           let longitude = props["mapLongitude"] as? NSNumber,
           let latitudeDelta = props["mapLatitudeDelta"] as? NSNumber,
           let longitudeDelta = props["mapLongitudeDelta"] as? NSNumber
        {
            let region = MKCoordinateRegion(
                center: CLLocationCoordinate2D(
                    latitude: latitude.doubleValue,
                    longitude: longitude.doubleValue
                ),
                span: MKCoordinateSpan(
                    latitudeDelta: latitudeDelta.doubleValue,
                    longitudeDelta: longitudeDelta.doubleValue
                )
            )
            map.setRegion(region, animated: false)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
                self?.map.setRegion(region, animated: false)
            }
        }

        guard
            let rawMarkers = props["mapMarkers"] as? String,
            let data = rawMarkers.data(using: .utf8),
            let markers = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            return
        }

        map.removeAnnotations(map.annotations.filter { $0 is MKPointAnnotation })
        for marker in markers {
            guard
                let latitude = marker["lat"] as? NSNumber,
                let longitude = marker["lng"] as? NSNumber
            else {
                continue
            }
            let annotation = MKPointAnnotation()
            annotation.coordinate = CLLocationCoordinate2D(
                latitude: latitude.doubleValue,
                longitude: longitude.doubleValue
            )
            annotation.title = marker["name"] as? String
            map.addAnnotation(annotation)
        }
    }

    func mapView(_ mapView: MKMapView, viewFor annotation: MKAnnotation) -> MKAnnotationView? {
        guard annotation is MKPointAnnotation else {
            return nil
        }
        let identifier = "cheetah-place"
        let marker = mapView.dequeueReusableAnnotationView(withIdentifier: identifier)
            as? MKMarkerAnnotationView
            ?? MKMarkerAnnotationView(annotation: annotation, reuseIdentifier: identifier)
        marker.annotation = annotation
        marker.markerTintColor = UIColor(hex: "#e5522f")
        marker.glyphImage = UIImage(systemName: "fork.knife")
        marker.canShowCallout = true
        marker.displayPriority = .required
        return marker
    }
}

private final class SheetNativeModule: CheetahNativeModule {
    private weak var owner: NativeModuleHostView?
    private var controller: UIViewController?

    var nativeView: UIView {
        UIView()
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
    }

    func apply(props: [String: Any]) {
        _ = props
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
                230
            }
            sheet.detents = [detent]
            sheet.selectedDetentIdentifier = identifier
            sheet.largestUndimmedDetentIdentifier = identifier
            sheet.prefersGrabberVisible = true
            sheet.preferredCornerRadius = 28
            sheet.prefersScrollingExpandsWhenScrolledToEdge = false
        }

        presenter.present(controller, animated: false)
        self.controller = controller
    }

    func dismiss() {
        controller?.dismiss(animated: false)
        controller = nil
    }

    func destroy() {
        dismiss()
    }
}
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
