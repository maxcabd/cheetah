import Photos
import PhotosUI
import StoreKit
import UIKit
import UniformTypeIdentifiers

/*
 Camera, photo library, documents, app files, and in-app purchases.
 Contracts: runtime::platform::{media, files, purchases}.
 */

/// Where picked and captured media are copied; Rust reads them with std::fs.
private func mediaDirectory() -> URL {
    let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
    let directory = caches.appendingPathComponent("cheetah-media", isDirectory: true)
    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
}

private func describe(_ url: URL, name: String? = nil) -> [String: Any] {
    let size = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.intValue ?? 0
    let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
    return ["path": url.path, "name": name ?? url.lastPathComponent, "mime": mime, "size": size]
}

/// Copy `source` into the media directory under a fresh name.
private func adopt(_ source: URL) -> URL? {
    let target = mediaDirectory().appendingPathComponent(
        UUID().uuidString + (source.pathExtension.isEmpty ? "" : "." + source.pathExtension)
    )
    do {
        try FileManager.default.copyItem(at: source, to: target)
        return target
    } catch {
        return nil
    }
}

final class MediaModule: NSObject, CheetahApiModule, PHPickerViewControllerDelegate,
    UIImagePickerControllerDelegate, UINavigationControllerDelegate, UIDocumentPickerDelegate
{
    static let moduleName = "media"
    private let context: ModuleContext
    private var pendingId = ""

    init(context: ModuleContext) {
        self.context = context
        super.init()
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        guard let presenter = context.topController else {
            return false
        }
        let id = args["id"] as? String ?? ""
        switch method {
        case "pickImages":
            pendingId = id
            var config = PHPickerConfiguration()
            config.filter = .images
            config.selectionLimit = max((args["limit"] as? NSNumber)?.intValue ?? 1, 1)
            let picker = PHPickerViewController(configuration: config)
            picker.delegate = self
            presenter.present(picker, animated: true)
        case "takePhoto":
            pendingId = id
            guard UIImagePickerController.isSourceTypeAvailable(.camera) else {
                emitFiles(id, [], error: "no camera")
                return true
            }
            let camera = UIImagePickerController()
            camera.sourceType = .camera
            camera.delegate = self
            presenter.present(camera, animated: true)
        case "pickDocuments":
            pendingId = id
            let mimes = args["types"] as? [String] ?? []
            let types = mimes.compactMap { UTType(mimeType: $0) }
            let picker = UIDocumentPickerViewController(
                forOpeningContentTypes: types.isEmpty ? [.item] : types,
                asCopy: true
            )
            picker.allowsMultipleSelection = true
            picker.delegate = self
            presenter.present(picker, animated: true)
        case "saveImage":
            saveImage(id, path: args["path"] as? String ?? "")
        default:
            return false
        }
        return true
    }

    private func emitFiles(_ id: String, _ files: [[String: Any]], error: String? = nil) {
        var values: [String: Any] = ["id": id, "files": files]
        if let error {
            values["error"] = error
        }
        context.emit("media", "result", values)
    }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        picker.dismiss(animated: true)
        let id = pendingId
        let group = DispatchGroup()
        let lock = NSLock()
        var files: [[String: Any]] = []
        for result in results {
            group.enter()
            let provider = result.itemProvider
            provider.loadFileRepresentation(forTypeIdentifier: UTType.image.identifier) { url, _ in
                // The provided file is deleted when this returns: copy now.
                if let url, let copy = adopt(url) {
                    lock.lock()
                    files.append(describe(copy, name: provider.suggestedName ?? url.lastPathComponent))
                    lock.unlock()
                }
                group.leave()
            }
        }
        group.notify(queue: .main) { [weak self] in
            self?.emitFiles(id, files)
        }
    }

    func imagePickerController(
        _ picker: UIImagePickerController,
        didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
    ) {
        picker.dismiss(animated: true)
        guard
            let image = info[.originalImage] as? UIImage,
            let data = image.jpegData(compressionQuality: 0.9)
        else {
            emitFiles(pendingId, [])
            return
        }
        let url = mediaDirectory().appendingPathComponent("photo-\(UUID().uuidString).jpg")
        do {
            try data.write(to: url)
            emitFiles(pendingId, [describe(url)])
        } catch {
            emitFiles(pendingId, [], error: error.localizedDescription)
        }
    }

    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        emitFiles(pendingId, [])
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        let files = urls.compactMap { url in adopt(url).map { describe($0, name: url.lastPathComponent) } }
        emitFiles(pendingId, files)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        emitFiles(pendingId, [])
    }

    private func saveImage(_ id: String, path: String) {
        let url = URL(fileURLWithPath: path)
        PHPhotoLibrary.requestAuthorization(for: .addOnly) { [context = self.context] status in
            guard status == .authorized || status == .limited else {
                context.emit("media", "saved", ["id": id, "ok": false, "error": "permission_denied"])
                return
            }
            PHPhotoLibrary.shared().performChanges({
                PHAssetChangeRequest.creationRequestForAssetFromImage(atFileURL: url)
            }) { ok, error in
                var values: [String: Any] = ["id": id, "ok": ok]
                if let error {
                    values["error"] = error.localizedDescription
                }
                context.emit("media", "saved", values)
            }
        }
    }
}

final class FilesModule: CheetahApiModule {
    static let moduleName = "files"
    private let context: ModuleContext

    init(context: ModuleContext) {
        self.context = context
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "paths":
            let manager = FileManager.default
            context.emit("files", "paths", [
                "documents": manager.urls(for: .documentDirectory, in: .userDomainMask)[0].path,
                "cache": manager.urls(for: .cachesDirectory, in: .userDomainMask)[0].path,
            ])
        case "share":
            guard let presenter = context.topController else {
                return false
            }
            let url = URL(fileURLWithPath: args["path"] as? String ?? "")
            let share = UIActivityViewController(activityItems: [url], applicationActivities: nil)
            if let popover = share.popoverPresentationController {
                popover.sourceView = presenter.view
                popover.sourceRect = CGRect(x: presenter.view.bounds.midX, y: presenter.view.bounds.maxY, width: 0, height: 0)
            }
            presenter.present(share, animated: true)
        default:
            return false
        }
        return true
    }
}

/// StoreKit 2. Verify `token` (the signed transaction) on your server before
/// granting anything.
final class PurchasesModule: CheetahApiModule {
    static let moduleName = "purchases"
    private let context: ModuleContext
    private var products: [String: Product] = [:]
    private var updates: Task<Void, Never>?

    init(context: ModuleContext) {
        self.context = context
        // Purchases that complete outside the flow (Ask to Buy, renewals).
        updates = Task { [weak self] in
            for await result in Transaction.updates {
                if case .verified(let transaction) = result {
                    self?.context.emit("purchases", "purchase", Self.describe(transaction, result.jwsRepresentation))
                }
            }
        }
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "products":
            let ids = args["ids"] as? [String] ?? []
            Task { await loadProducts(ids) }
        case "purchase":
            let id = args["id"] as? String ?? ""
            Task { await purchase(id) }
        case "restore":
            Task { await restore() }
        case "finish":
            let transaction = args["transaction"] as? String ?? ""
            Task { await finish(transaction) }
        default:
            return false
        }
        return true
    }

    private static func describe(_ transaction: Transaction, _ token: String) -> [String: Any] {
        ["id": transaction.productID, "transaction": String(transaction.id), "token": token]
    }

    @MainActor
    private func loadProducts(_ ids: [String]) async {
        do {
            let found = try await Product.products(for: ids)
            for product in found {
                products[product.id] = product
            }
            context.emit("purchases", "products", [
                "products": found.map {
                    ["id": $0.id, "title": $0.displayName, "description": $0.description, "price": $0.displayPrice]
                },
            ])
        } catch {
            context.emit("purchases", "products", ["products": [], "error": error.localizedDescription])
        }
    }

    @MainActor
    private func purchase(_ id: String) async {
        guard let product = products[id] else {
            context.emit("purchases", "purchase", ["id": id, "error": "load it with purchases.products first"])
            return
        }
        do {
            switch try await product.purchase() {
            case .success(let result):
                switch result {
                case .verified(let transaction):
                    context.emit("purchases", "purchase", Self.describe(transaction, result.jwsRepresentation))
                case .unverified(_, let error):
                    context.emit("purchases", "purchase", ["id": id, "error": "unverified: \(error.localizedDescription)"])
                }
            case .userCancelled:
                context.emit("purchases", "purchase", ["id": id, "cancelled": true])
            case .pending:
                context.emit("purchases", "purchase", ["id": id, "pending": true])
            @unknown default:
                context.emit("purchases", "purchase", ["id": id, "error": "unknown result"])
            }
        } catch {
            context.emit("purchases", "purchase", ["id": id, "error": error.localizedDescription])
        }
    }

    @MainActor
    private func restore() async {
        try? await AppStore.sync()
        var purchases: [[String: Any]] = []
        for await result in Transaction.currentEntitlements {
            if case .verified(let transaction) = result {
                purchases.append(Self.describe(transaction, result.jwsRepresentation))
            }
        }
        context.emit("purchases", "restore", ["purchases": purchases])
    }

    @MainActor
    private func finish(_ id: String) async {
        for await result in Transaction.unfinished {
            if case .verified(let transaction) = result, String(transaction.id) == id {
                await transaction.finish()
                context.emit("purchases", "finished", ["transaction": id, "ok": true])
                return
            }
        }
        context.emit("purchases", "finished", ["transaction": id, "ok": false])
    }

    func dispose() {
        updates?.cancel()
    }
}
