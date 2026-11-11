import CoreText
import MapKit
import UIKit

private let cheetahOpsCallback: @convention(c) (
    UnsafePointer<CChar>?,
    UnsafeMutableRawPointer?
) -> Void = { json, context in
    guard let json, let context else {
        return
    }
    let hostContext = Unmanaged<HostContext>.fromOpaque(context).takeUnretainedValue()
    hostContext.host?.onOps(String(cString: json))
}

private let cheetahMeasureCallback: @convention(c) (
    UnsafePointer<CChar>?,
    Float,
    Int32,
    Float,
    UnsafeMutablePointer<Float>?,
    UnsafeMutablePointer<Float>?,
    UnsafeMutableRawPointer?
) -> Void = { text, size, bold, maxWidth, outWidth, outHeight, context in
    guard let text, let outWidth, let outHeight, let context else {
        return
    }
    let hostContext = Unmanaged<HostContext>.fromOpaque(context).takeUnretainedValue()
    let measured = hostContext.measure(
        text: String(cString: text),
        size: CGFloat(size),
        bold: bold != 0,
        maxWidth: CGFloat(maxWidth)
    )
    outWidth.pointee = Float(measured.width)
    outHeight.pointee = Float(measured.height)
}

private final class HostContext {
    weak var host: RustHost?

    func measure(text: String, size: CGFloat, bold: Bool, maxWidth: CGFloat) -> CGSize {
        let font = UIFont.systemFont(ofSize: max(size, 1), weight: bold ? .bold : .regular)
        let attributed = NSAttributedString(string: text, attributes: [.font: font])
        let framesetter = CTFramesetterCreateWithAttributedString(attributed as CFAttributedString)
        var fitRange = CFRange()
        let limit = max(maxWidth, 1)
        let measured = CTFramesetterSuggestFrameSizeWithConstraints(
            framesetter,
            CFRange(location: 0, length: 0),
            nil,
            CGSize(width: limit, height: .greatestFiniteMagnitude),
            &fitRange
        )
        return CGSize(
            width: ceil(min(measured.width, limit)) + 1,
            height: ceil(measured.height)
        )
    }
}

private let cheetahImageCache = NSCache<NSURL, UIImage>()

private final class NodeTag {
    let id: UInt32
    let kind: String

    weak var view: UIView?
    var container: UIView?
    weak var scrollView: UIScrollView?
    var contentHeight: CGFloat = 0
    var contentWidth: CGFloat = 0
    var pendingScrollTo: CGFloat?
    var lastScrollTo: CGFloat?
    var lastValue: String?
    var updating = false
    var mapProps: [String: Any]?
    var imageSource: String?
    var tapRecognizer: UITapGestureRecognizer?
    var source: String?
    var native: String?
    var lastPage: Int = -1

    var isSheet: Bool {
        kind == "sheet" || native == "sheet"
    }

    init(id: UInt32, kind: String, view: UIView) {
        self.id = id
        self.kind = kind
        self.view = view
    }
}

final class RustHost: NSObject {
    private let container: UIView
    private let context = HostContext()
    private let contextPointer: UnsafeMutableRawPointer
    private var handle: Int64 = 0
    private var disposed = false
    private var lastSize: CGSize = .zero
    private var lastSafeTop: CGFloat = -1
    private var lastSafeBottom: CGFloat = -1
    private var lastKeyboardInset: CGFloat = -1
    private var lastGlass: Bool?
    private var views: [UInt32: UIView] = [:]
    private var tags: [UInt32: NodeTag] = [:]
    private var scrollTags: [ObjectIdentifier: NodeTag] = [:]
    private var scrollScheduled: Set<ObjectIdentifier> = []
    private var sheetController: UIViewController?

    var containerSize: CGSize {
        container.bounds.size
    }

    init(container: UIView) {
        self.container = container
        contextPointer = Unmanaged.passRetained(context).toOpaque()
        super.init()

        context.host = self

        let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let storePath = documents.appendingPathComponent("cheetah.redb").path
        let arguments = ProcessInfo.processInfo.arguments
        let app: String
        if arguments.contains("--nightshade-demo") {
            app = "nightshade-demo"
        } else if arguments.contains("--nightshade") {
            app = "nightshade"
        } else {
            app = "chat"
        }

        handle = storePath.withCString { store in
            "ws://127.0.0.1:9001".withCString { gateway in
                app.withCString { appName in
                    cheetah_new_app(
                        store,
                        gateway,
                        appName,
                        cheetahOpsCallback,
                        cheetahMeasureCallback,
                        contextPointer
                    )
                }
            }
        }
    }

    func updateHostInfo(
        safeTop: CGFloat,
        safeBottom: CGFloat,
        keyboardInset: CGFloat,
        glass: Bool
    ) {
        guard handle != 0 else {
            return
        }

        let safeTop = max(safeTop, 0)
        let safeBottom = max(safeBottom, 0)
        let keyboardInset = max(keyboardInset, 0)

        if abs(safeTop - lastSafeTop) < 0.5,
           abs(safeBottom - lastSafeBottom) < 0.5,
           abs(keyboardInset - lastKeyboardInset) < 0.5,
           lastGlass == glass
        {
            return
        }
        lastSafeTop = safeTop
        lastSafeBottom = safeBottom
        lastKeyboardInset = keyboardInset
        lastGlass = glass

        let info: [String: Any] = [
            "safeTop": Double(safeTop),
            "safeBottom": Double(safeBottom),
            "keyboardInset": Double(keyboardInset),
            "overlayChrome": true,
            "glass": glass,
            "scale": Double(UIScreen.main.scale),
        ]

        guard
            let data = try? JSONSerialization.data(withJSONObject: info),
            let json = String(data: data, encoding: .utf8)
        else {
            return
        }

        "host".withCString { kind in
            json.withCString { payload in
                cheetah_event(handle, 0, kind, payload)
            }
        }
    }

    func setActive(_ active: Bool) {
        guard handle != 0, !disposed else {
            return
        }
        cheetah_lifecycle(handle, active ? 1 : 0)
    }

    func resize(to size: CGSize) {
        guard handle != 0, size.width > 0, size.height > 0 else {
            return
        }
        if abs(size.width - lastSize.width) < 0.5, abs(size.height - lastSize.height) < 0.5 {
            return
        }
        lastSize = size
        cheetah_resize(handle, Float(size.width), Float(size.height))
    }

    func dispose() {
        guard !disposed else {
            return
        }
        disposed = true

        if handle != 0 {
            cheetah_free(handle)
            handle = 0
        }

        sheetController?.dismiss(animated: false)
        sheetController = nil
        for view in views.values {
            if let native = view as? NativeModuleHostView {
                native.destroy()
            }
        }
        context.host = nil
        Unmanaged<HostContext>.fromOpaque(contextPointer).release()
    }

    deinit {
        dispose()
    }

    fileprivate func onOps(_ json: String) {
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.disposed else {
                return
            }
            self.applyOps(json)
        }
    }

    private func applyOps(_ json: String) {
        guard
            let data = json.data(using: .utf8),
            let ops = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            return
        }

        for op in ops {
            switch op["op"] as? String {
            case "create":
                create(op)
            case "update":
                update(op)
            case "children":
                children(op)
            case "layout":
                layout(op)
            case "remove":
                remove(op)
            default:
                break
            }
        }
    }

    private func create(_ op: [String: Any]) {
        let id = uint32(op["id"])
        let kind = op["kind"] as? String ?? "space"
        let view = makeView(kind)
        view.tag = Int(id)

        let node = NodeTag(id: id, kind: kind, view: view)
        let nodeProps = op["props"] as? [String: Any] ?? [:]
        node.native = nodeProps["native"] as? String
        if let source = op["source"] as? [String: Any],
           let file = source["file"] as? String,
           let line = source["line"] as? NSNumber
        {
            node.source = "\(file):\(line.intValue)"
            view.accessibilityHint = node.source
        }
        if let scroll = view as? UIScrollView, let content = scroll.subviews.first {
            node.container = content
            node.scrollView = scroll
            scrollTags[ObjectIdentifier(scroll)] = node
        } else if let glass = view as? GlassView {
            node.container = glass.contentView
        } else if kind == "column" || kind == "row" || kind == "sheet" || node.isSheet {
            node.container = view
        }

        views[id] = view
        tags[id] = node

        if bool(op["root"]) {
            container.addSubview(view)
        }

        applyProps(view, node: node, props: nodeProps)
    }

    private func makeView(_ kind: String) -> UIView {
        switch kind {
        case "text":
            let label = InsetLabel()
            label.numberOfLines = 0
            label.textColor = .white
            return label

        case "button":
            let button = CheetahButton()
            button.addTarget(self, action: #selector(buttonTapped(_:)), for: .touchUpInside)
            return button

        case "input":
            let field = InsetTextField()
            field.borderStyle = .none
            field.textColor = .white
            field.autocapitalizationType = .sentences
            field.returnKeyType = .send
            field.addTarget(self, action: #selector(textChanged(_:)), for: .editingChanged)
            field.delegate = self
            return field

        case "list", "scroll":
            let scroll = UIScrollView()
            scroll.showsVerticalScrollIndicator = true
            scroll.alwaysBounceVertical = true
            scroll.keyboardDismissMode = .interactive
            scroll.delegate = self
            scroll.clipsToBounds = true
            scroll.contentInsetAdjustmentBehavior = .never
            let content = UIView()
            scroll.addSubview(content)
            return scroll

        case "hscroll":
            let scroll = UIScrollView()
            scroll.showsHorizontalScrollIndicator = false
            scroll.showsVerticalScrollIndicator = false
            scroll.alwaysBounceHorizontal = true
            scroll.delegate = self
            scroll.clipsToBounds = true
            scroll.contentInsetAdjustmentBehavior = .never
            let content = UIView()
            scroll.addSubview(content)
            return scroll

        case "image":
            let imageView = UIImageView()
            imageView.contentMode = .scaleAspectFill
            imageView.clipsToBounds = true
            imageView.backgroundColor = UIColor(hex: "#242424")
            imageView.isUserInteractionEnabled = false
            return imageView

        case "native":
            return NativeModuleHostView()

        case "glass":
            return GlassView()

        case "map":
            let initialFrame = container.bounds.width > 0 && container.bounds.height > 0
                ? container.bounds
                : CGRect(x: 0, y: 0, width: 1, height: 1)
            let map = MKMapView(frame: initialFrame)
            map.backgroundColor = UIColor(hex: "#101826")
            map.isOpaque = true
            map.showsCompass = false
            map.showsScale = false
            map.isRotateEnabled = false
            map.isPitchEnabled = false
            map.delegate = self
            return map

        default:
            let view = UIView()
            view.clipsToBounds = true
            return view
        }
    }

    private func update(_ op: [String: Any]) {
        let id = uint32(op["id"])
        guard let view = views[id], let node = tags[id] else {
            return
        }
        applyProps(view, node: node, props: op["props"] as? [String: Any] ?? [:])
    }

    private func children(_ op: [String: Any]) {
        let id = uint32(op["id"])
        // Leaf views such as MKMapView, UITextField and UIButton own internal
        // subviews. Only nodes that opted into a container may be pruned.
        guard let node = tags[id], let parent = node.container else {
            return
        }

        let numbers = op["children"] as? [NSNumber] ?? []
        let wanted = numbers.map { UInt32(truncating: $0) }
        let wantedViews = Set(wanted.compactMap { views[$0] })

        for subview in parent.subviews where !wantedViews.contains(subview) {
            subview.removeFromSuperview()
        }

        for (index, childId) in wanted.enumerated() {
            guard let child = views[childId] else {
                continue
            }
            if let childNode = tags[childId], childNode.isSheet {
                if childNode.kind == "sheet" {
                    presentSheetIfNeeded(child)
                }
                continue
            }
            if child.superview !== parent {
                parent.addSubview(child)
            }
            let target = min(index, max(0, parent.subviews.count - 1))
            parent.insertSubview(child, at: target)
        }

        if node.scrollView != nil {
            updateScrollContent(node)
        }
    }

    private func layout(_ op: [String: Any]) {
        let id = uint32(op["id"])
        guard let view = views[id] else {
            return
        }
        if tags[id]?.isSheet == true {
            return
        }

        view.frame = CGRect(
            x: number(op["x"]),
            y: number(op["y"]),
            width: number(op["width"]),
            height: number(op["height"])
        )

        if let node = tags[id], node.scrollView != nil {
            updateScrollContent(node)
            applyScroll(node)
        }

        if let map = view as? MKMapView, let node = tags[id] {
            map.setNeedsLayout()
            map.layoutIfNeeded()
            if let props = node.mapProps {
                applyMap(map, props: props)
            }
        }
    }

    private func presentSheetIfNeeded(_ view: UIView) {
        guard sheetController == nil else {
            return
        }
        guard let presenter = container.window?.rootViewController else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self, weak view] in
                guard let self, let view else {
                    return
                }
                self.presentSheetIfNeeded(view)
            }
            return
        }

        let controller = UIViewController()
        controller.view.backgroundColor = UIColor(hex: "#171717")
        controller.isModalInPresentation = true
        view.translatesAutoresizingMaskIntoConstraints = false
        controller.view.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: controller.view.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: controller.view.trailingAnchor),
            view.topAnchor.constraint(equalTo: controller.view.topAnchor),
            view.bottomAnchor.constraint(equalTo: controller.view.bottomAnchor),
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
        sheetController = controller
    }

    private func remove(_ op: [String: Any]) {
        let id = uint32(op["id"])
        if tags[id]?.isSheet == true {
            sheetController?.dismiss(animated: false)
            sheetController = nil
        }
        guard let view = views.removeValue(forKey: id) else {
            return
        }
        if let scroll = view as? UIScrollView {
            scrollTags.removeValue(forKey: ObjectIdentifier(scroll))
        }
        if let native = view as? NativeModuleHostView {
            native.destroy()
        }
        view.removeFromSuperview()
        tags.removeValue(forKey: id)
    }

    private func applyProps(_ view: UIView, node: NodeTag, props: [String: Any]) {
        let text = props["text"] as? String
        let size = max(number(props["size"], fallback: 14), 1)
        let bold = bool(props["bold"])
        let fontFamily = props["font"] as? String
        let font = uiFont(size: size, bold: bold, family: fontFamily)

        if let label = view as? UILabel {
            if let text {
                label.text = text
            }
            label.font = font
            if let color = props["color"] as? String {
                label.textColor = UIColor(hex: color)
            }
            if let align = props["align"] as? String {
                label.textAlignment = textAlignment(align)
            }
            if let lines = props["lines"] as? NSNumber {
                let count = lines.intValue
                label.numberOfLines = count
                label.lineBreakMode = count == 1 ? .byTruncatingTail : .byWordWrapping
            }
            if let lineHeight = props["lineHeight"] as? NSNumber, let text {
                let paragraph = NSMutableParagraphStyle()
                paragraph.minimumLineHeight = CGFloat(lineHeight.doubleValue)
                paragraph.maximumLineHeight = CGFloat(lineHeight.doubleValue)
                paragraph.alignment = label.textAlignment
                paragraph.lineBreakMode = label.lineBreakMode
                label.attributedText = NSAttributedString(
                    string: text,
                    attributes: [
                        .font: font,
                        .foregroundColor: label.textColor ?? .white,
                        .paragraphStyle: paragraph,
                    ]
                )
            }
        }

        if let button = view as? CheetahButton {
            if let text {
                button.title = text
            }
            button.titleFont = font
            if let color = props["color"] as? String {
                button.titleColor = UIColor(hex: color)
            }
            if let align = props["align"] as? String {
                button.titleAlignment = textAlignment(align)
            }
        }

        if let field = view as? UITextField {
            field.font = font
            if let color = props["color"] as? String {
                field.textColor = UIColor(hex: color)
            }
            if let placeholder = props["placeholder"] as? String {
                field.attributedPlaceholder = NSAttributedString(
                    string: placeholder,
                    attributes: [
                        .foregroundColor: UIColor(
                            hex: props["placeholderColor"] as? String ?? "#a3a3a3"
                        ),
                    ]
                )
            }
            if let align = props["align"] as? String {
                field.textAlignment = textAlignment(align)
            }
            if let value = props["value"] as? String {
                let previous = node.lastValue
                node.lastValue = value
                if field.isFirstResponder {
                    // Do not echo a stale Rust value back into the field while
                    // the user is typing ahead of the actor. An explicit clear
                    // after send is the one update we must apply immediately.
                    if let previous, !previous.isEmpty, value.isEmpty {
                        node.updating = true
                        field.text = value
                        node.updating = false
                    }
                } else if field.text != value {
                    node.updating = true
                    field.text = value
                    node.updating = false
                }
            }
        }

        if bool(props["pressable"]), node.tapRecognizer == nil {
            view.isUserInteractionEnabled = true
            let tap = UITapGestureRecognizer(target: self, action: #selector(viewTapped(_:)))
            view.addGestureRecognizer(tap)
            node.tapRecognizer = tap
        }

        if let imageView = view as? UIImageView, let source = props["src"] as? String {
            if node.imageSource != source {
                node.imageSource = source
                imageView.image = nil
                loadImage(source, into: imageView, node: node)
            }
        }

        if let imageView = view as? UIImageView, let fit = props["fit"] as? String {
            imageView.contentMode = fit == "contain" ? .scaleAspectFit : .scaleAspectFill
        }

        if let native = view as? NativeModuleHostView {
            native.onEvent = { [weak self] kind, payload in
                self?.event(id: node.id, kind: kind, payload: payload)
            }
            native.configure(
                name: props["native"] as? String ?? "",
                props: props
            )
        }

        if let background = props["bg"] as? String {
            view.backgroundColor = UIColor(hex: background)
        }

        if let glass = view as? GlassView {
            glass.apply(
                style: props["glassStyle"] as? String ?? "regular",
                tint: props["glassTint"] as? String,
                interactive: bool(props["glassInteractive"])
            )
        }

        if let map = view as? MKMapView {
            node.mapProps = props
            // A map created during the create pass has no superview yet. Let
            // the first layout pass install it in the window before MapKit
            // initializes its Metal-backed map layer.
            if map.superview != nil {
                applyMap(map, props: props)
            }
        }

        let radius = number(props["radius"])
        view.layer.cornerRadius = radius
        if let scroll = view as? UIScrollView {
            // Rows include overscan outside the viewport. UIScrollView must
            // clip even when it has no corner radius.
            scroll.clipsToBounds = true
        } else if let glass = view as? GlassView {
            glass.updateShape(radius: radius)
        } else {
            view.clipsToBounds = bool(props["clip"]) || radius > 0
        }

        let borderWidth = number(props["borderWidth"])
        if borderWidth > 0 {
            view.layer.borderWidth = borderWidth
            view.layer.borderColor = UIColor(
                hex: props["borderColor"] as? String ?? "#ffffff"
            ).cgColor
        } else {
            view.layer.borderWidth = 0
        }
        view.alpha = props["opacity"] != nil ? CGFloat(number(props["opacity"])) : 1.0
        if bool(props["shadow"]) {
            view.layer.shadowColor = UIColor.black.cgColor
            view.layer.shadowOpacity = 0.3
            view.layer.shadowRadius = 8
            view.layer.shadowOffset = CGSize(width: 0, height: 4)
            view.layer.masksToBounds = false
        } else {
            view.layer.shadowOpacity = 0
        }

        let base = number(props["pad"])
        let horizontal = number(props["padH"], fallback: base)
        let vertical = number(props["padV"], fallback: base)
        let insets = UIEdgeInsets(top: vertical, left: horizontal, bottom: vertical, right: horizontal)
        if let label = view as? InsetLabel {
            label.insets = insets
        }
        if let field = view as? InsetTextField {
            field.insets = insets
        }
        if let button = view as? CheetahButton {
            button.insets = insets
        }

        if let label = props["label"] as? String {
            view.accessibilityLabel = label
            view.isAccessibilityElement = true
            view.accessibilityTraits = .button
        }

        if let testID = props["testId"] as? String {
            view.accessibilityIdentifier = testID
        }

        if let scroll = view as? UIScrollView {
            if props["contentInsetTop"] != nil || props["contentInsetBottom"] != nil {
                let top = props["contentInsetTop"] != nil
                    ? number(props["contentInsetTop"])
                    : scroll.contentInset.top
                let bottom = props["contentInsetBottom"] != nil
                    ? number(props["contentInsetBottom"])
                    : scroll.contentInset.bottom
                scroll.contentInset = UIEdgeInsets(
                    top: top,
                    left: 0,
                    bottom: bottom,
                    right: 0
                )
            }
            if let contentHeight = props["contentHeight"] {
                node.contentHeight = number(contentHeight)
                updateScrollContent(node)
            }
            if let contentWidth = props["contentWidth"] {
                node.contentWidth = number(contentWidth)
                updateScrollContent(node)
            }
            if let scrollTo = props["scrollTo"] {
                node.pendingScrollTo = number(scrollTo)
                applyScroll(node)
            }
            if let paged = props["paged"] as? NSNumber {
                scroll.isPagingEnabled = paged.boolValue
            }
            if let scrollX = props["scrollX"] as? NSNumber {
                scroll.setContentOffset(
                    CGPoint(x: CGFloat(scrollX.doubleValue), y: scroll.contentOffset.y),
                    animated: false
                )
            }
        }
    }

    private func uiFont(size: CGFloat, bold: Bool, family: String?) -> UIFont {
        let name: String?
        switch family {
        case "satoshi": name = "Satoshi-Regular"
        case "satoshi-medium": name = "Satoshi-Medium"
        case "satoshi-bold": name = "Satoshi-Bold"
        case "brice": name = "Brice-SemiBold"
        case "brice-semibold": name = "Brice-SemiBold"
        case "brice-bold": name = "Brice-Bold"
        case "general": name = "GeneralSans-Regular"
        case "general-medium": name = "GeneralSans-Medium"
        case "general-semibold": name = "GeneralSans-Semibold"
        case "general-bold": name = "GeneralSans-Bold"
        default: name = nil
        }
        if let name, let font = UIFont(name: name, size: size) {
            return font
        }
        return UIFont.systemFont(ofSize: size, weight: bold ? .bold : .regular)
    }

    private func applyMap(_ map: MKMapView, props: [String: Any]) {
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
            // MapKit may discard an early region update while the view is
            // still zero-sized. Re-apply once the first layout pass is done.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak map] in
                map?.setRegion(region, animated: false)
            }
        }

        guard
            let rawMarkers = props["mapMarkers"] as? String,
            let data = rawMarkers.data(using: .utf8),
            let markers = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else {
            return
        }

        let oldAnnotations = map.annotations.filter { $0 is MKPointAnnotation }
        map.removeAnnotations(oldAnnotations)

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

    private func updateScrollContent(_ node: NodeTag) {
        guard let scroll = node.scrollView, let content = node.container else {
            return
        }

        if node.kind == "hscroll" {
            let width = max(node.contentWidth, scroll.bounds.width, 1)
            let height = max(scroll.bounds.height, 1)
            content.frame = CGRect(x: 0, y: 0, width: width, height: height)
            scroll.contentSize = CGSize(width: width, height: height)
            return
        }

        let width = max(scroll.bounds.width, 1)
        let height = max(node.contentHeight, 1)
        content.frame = CGRect(x: 0, y: 0, width: width, height: height)
        scroll.contentSize = CGSize(width: width, height: height)
    }

    private func loadImage(_ source: String, into imageView: UIImageView, node: NodeTag) {
        if source.hasPrefix("asset://") {
            let name = String(source.dropFirst("asset://".count))
            imageView.image = UIImage(named: name)
            return
        }

        guard let url = URL(string: source) else {
            return
        }

        if let cached = cheetahImageCache.object(forKey: url as NSURL) {
            imageView.image = cached
            return
        }

        URLSession.shared.dataTask(with: url) { [weak self, weak imageView] data, _, _ in
            guard let data, let image = UIImage(data: data) else {
                return
            }
            cheetahImageCache.setObject(image, forKey: url as NSURL)
            DispatchQueue.main.async {
                guard
                    let self,
                    let imageView,
                    self.tags[UInt32(imageView.tag)]?.imageSource == source
                else {
                    return
                }
                imageView.image = image
                node.imageSource = source
            }
        }.resume()
    }

    private func applyScroll(_ node: NodeTag) {
        guard let scroll = node.scrollView, let target = node.pendingScrollTo, scroll.bounds.height > 0 else {
            return
        }

        let contentOffset = target - scroll.contentInset.top
        node.lastScrollTo = target
        scroll.setContentOffset(CGPoint(x: 0, y: contentOffset), animated: false)
        if abs(scroll.contentOffset.y - contentOffset) <= 1 {
            node.pendingScrollTo = nil
        }
    }

    @objc private func buttonTapped(_ sender: CheetahButton) {
        event(id: UInt32(sender.tag), kind: "click", payload: "")
    }

    @objc private func viewTapped(_ sender: UITapGestureRecognizer) {
        guard let view = sender.view else {
            return
        }
        container.endEditing(true)
        event(id: UInt32(view.tag), kind: "click", payload: "")
    }

    @objc private func textChanged(_ sender: UITextField) {
        let id = UInt32(sender.tag)
        guard let node = tags[id], !node.updating else {
            return
        }
        event(id: id, kind: "change", payload: sender.text ?? "")
    }

    private func event(id: UInt32, kind: String, payload: String) {
        guard handle != 0 else {
            return
        }
        kind.withCString { kindPointer in
            payload.withCString { payloadPointer in
                cheetah_event(handle, id, kindPointer, payloadPointer)
            }
        }
    }

    private func number(_ value: Any?, fallback: CGFloat = 0) -> CGFloat {
        if let number = value as? NSNumber {
            return CGFloat(number.doubleValue)
        }
        return fallback
    }

    private func uint32(_ value: Any?) -> UInt32 {
        if let number = value as? NSNumber {
            return number.uint32Value
        }
        return 0
    }

    private func bool(_ value: Any?) -> Bool {
        if let number = value as? NSNumber {
            return number.boolValue
        }
        return false
    }

    private func textAlignment(_ align: String) -> NSTextAlignment {
        switch align {
        case "center":
            return .center
        case "end":
            return .right
        default:
            return .left
        }
    }

    private func contentAlignment(_ align: String) -> UIControl.ContentHorizontalAlignment {
        switch align {
        case "center":
            return .center
        case "end":
            return .right
        default:
            return .left
        }
    }
}

extension RustHost: UIScrollViewDelegate {
    func scrollViewDidEndDecelerating(_ scrollView: UIScrollView) {
        emitHorizontalPage(scrollView)
    }

    func scrollViewDidEndScrollingAnimation(_ scrollView: UIScrollView) {
        emitHorizontalPage(scrollView)
    }

    private func emitHorizontalPage(_ scrollView: UIScrollView) {
        let key = ObjectIdentifier(scrollView)
        guard let node = scrollTags[key], node.kind == "hscroll", scrollView.bounds.width > 0 else {
            return
        }
        let page = Int(round(scrollView.contentOffset.x / scrollView.bounds.width))
        guard page != node.lastPage else {
            return
        }
        node.lastPage = page
        event(id: node.id, kind: "page", payload: String(page))
    }

    func scrollViewDidScroll(_ scrollView: UIScrollView) {
        let key = ObjectIdentifier(scrollView)
        guard let node = scrollTags[key], node.pendingScrollTo == nil else {
            return
        }

        let offset = scrollView.contentOffset.y + scrollView.contentInset.top
        if let last = node.lastScrollTo, abs(offset - last) < 1 {
            return
        }
        if scrollScheduled.contains(key) {
            return
        }

        scrollScheduled.insert(key)
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(16)) { [weak self, weak scrollView] in
            guard let self, let scrollView else {
                return
            }
            self.scrollScheduled.remove(key)
            guard let node = self.scrollTags[key], node.pendingScrollTo == nil else {
                return
            }
            self.event(
                id: node.id,
                kind: "scroll",
                payload: String(Double(scrollView.contentOffset.y + scrollView.contentInset.top))
            )
        }
    }
}

extension RustHost: MKMapViewDelegate {
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

extension RustHost: UITextFieldDelegate {
    func textFieldDidBeginEditing(_ textField: UITextField) {
        let id = UInt32(textField.tag)
        guard tags[id] != nil else {
            return
        }
        event(id: id, kind: "focus", payload: "")
    }

    func textFieldDidEndEditing(_ textField: UITextField) {
        let id = UInt32(textField.tag)
        guard tags[id] != nil else {
            return
        }
        event(id: id, kind: "blur", payload: "")
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        let id = UInt32(textField.tag)
        guard tags[id] != nil else {
            return true
        }
        event(id: id, kind: "submit", payload: "")
        return true
    }
}
