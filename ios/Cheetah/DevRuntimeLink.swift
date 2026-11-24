import Foundation

/// Dev builds (Debug, or Release with CHEETAH_DEV_CLIENT for TestFlight):
/// connect RustHost to tools/devserver instead of the linked runtime. The
/// server comes from `--dev-server ws://<ip>:8787`, or from scanning the QR
/// code `cheetah dev` prints (`cheetah://dev?url=...`), which is remembered.
enum DevRuntimeLinks {
    private static let savedKey = "CheetahDevServer"

    static func make(host: RustHost, url: String) -> RuntimeLink? {
        #if DEBUG || CHEETAH_DEV_CLIENT
        guard let url = URL(string: url) else { return nil }
        return DevRuntimeLink(host: host, url: url)
        #else
        return nil
        #endif
    }

    /// The dev server from the last `cheetah://dev` link, if any.
    static var savedServer: String? {
        #if DEBUG || CHEETAH_DEV_CLIENT
        return UserDefaults.standard.string(forKey: savedKey)
        #else
        return nil
        #endif
    }

    /// Remember the server in a `cheetah://dev?url=ws://...` link (without
    /// `url`, forget it). True when `url` was a dev link.
    static func handle(_ url: URL) -> Bool {
        #if DEBUG || CHEETAH_DEV_CLIENT
        guard url.scheme == "cheetah", url.host == "dev" else { return false }
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        let server = items.first(where: { $0.name == "url" })?.value
        if let server, !server.isEmpty {
            UserDefaults.standard.set(server, forKey: savedKey)
        } else {
            UserDefaults.standard.removeObject(forKey: savedKey)
        }
        return true
        #else
        return false
        #endif
    }
}

#if DEBUG || CHEETAH_DEV_CLIENT
/// RuntimeLink over WebSocket (protocol in tools/devserver/src/main.rs).
/// Reconnects automatically; every (re)connection starts a fresh app on the
/// server, so the host clears its views and re-sends its size and insets.
private final class DevRuntimeLink: NSObject, RuntimeLink, URLSessionWebSocketDelegate {
    private weak var host: RustHost?
    private let url: URL
    private var session: URLSession!
    private var task: URLSessionWebSocketTask?
    private var open = false
    private var freed = false
    private var announcedWaiting = false

    init(host: RustHost, url: URL) {
        self.host = host
        self.url = url
        super.init()
        session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        connect()
    }

    private func connect() {
        guard !freed else { return }
        let task = session.webSocketTask(with: url)
        self.task = task
        task.resume()
        receive(on: task)
    }

    private func receive(on task: URLSessionWebSocketTask) {
        task.receive { [weak self] result in
            guard let self, task === self.task else { return }
            switch result {
            case .success(.data(let data)):
                self.host?.receiveDevOps(data)
            case .success(.string(let text)):
                self.answerMeasure(text, on: task)
            case .success:
                break
            case .failure(let error):
                if !self.announcedWaiting {
                    print("CheetahDev: waiting for dev server at \(self.url) (\(error.localizedDescription))")
                    self.announcedWaiting = true
                }
                self.retry()
                return
            }
            self.receive(on: task)
        }
    }

    private func answerMeasure(_ text: String, on task: URLSessionWebSocketTask) {
        guard
            let data = text.data(using: .utf8),
            let message = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            message["t"] as? String == "measure",
            let seq = message["seq"] as? NSNumber,
            let items = message["items"] as? [[Any]],
            let host
        else {
            return
        }
        let requests: [(String, CGFloat, Bool, CGFloat)] = items.map { item in
            (
                item.count > 0 ? (item[0] as? String ?? "") : "",
                CGFloat((item.count > 1 ? item[1] as? NSNumber : nil)?.doubleValue ?? 14),
                (item.count > 2 ? item[2] as? Bool : nil) ?? false,
                CGFloat((item.count > 3 ? item[3] as? NSNumber : nil)?.doubleValue ?? 1)
            )
        }
        let sizes = host.measureForDev(requests).map { Double($0) }
        send(["t": "measured", "seq": seq, "sizes": sizes], on: task)
    }

    private func retry() {
        open = false
        task = nil
        guard !freed else { return }
        // Short: the server is usually back within moments of a rebuild.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            self?.connect()
        }
    }

    func urlSession(
        _ session: URLSession,
        webSocketTask: URLSessionWebSocketTask,
        didOpenWithProtocol protocol: String?
    ) {
        guard webSocketTask === task else { return }
        open = true
        announcedWaiting = false
        print("CheetahDev: connected to \(url)")
        // Lets the dev server say when native code changed since this build.
        let native = Bundle.main.object(forInfoDictionaryKey: "CheetahNativeFingerprint") as? String
        send(["t": "hello", "platform": "ios", "native": native ?? "unknown"], on: webSocketTask)
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.freed else { return }
            self.host?.onRuntimeConnected()
        }
    }

    private func send(_ object: [String: Any], on task: URLSessionWebSocketTask? = nil) {
        guard
            let task = task ?? (open ? self.task : nil),
            let data = try? JSONSerialization.data(withJSONObject: object),
            let text = String(data: data, encoding: .utf8)
        else {
            return
        }
        task.send(.string(text)) { _ in }
    }

    func resize(width: Float, height: Float) {
        send(["t": "resize", "w": Double(width), "h": Double(height)])
    }

    func event(id: UInt32, kind: String, payload: String) {
        send(["t": "event", "id": id, "kind": kind, "payload": payload])
    }

    func eventCode(id: UInt32, code: UInt32, payload: String) {
        send(["t": "event", "id": id, "code": code, "payload": payload])
    }

    func lifecycle(active: Bool) {
        send(["t": "lifecycle", "active": active])
    }

    func applied(_ report: [String: Any]) {
        var message = report
        message["t"] = "applied"
        send(message)
    }

    func free() {
        freed = true
        task?.cancel(with: .normalClosure, reason: nil)
        task = nil
        session.invalidateAndCancel()
    }
}
#endif
