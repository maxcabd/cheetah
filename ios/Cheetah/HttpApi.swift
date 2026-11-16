import Foundation

/**
 `http` module (Rust: `runtime::http`).

 `request` args: `{id, method, url, headers, body?, timeoutMs}`.
 Answers with `{"module":"http","method":"response","id",ok,status,headers,body,error?}`.
 `ok` is transport success; 4xx/5xx still arrive with `ok: true`.
 Called and answered on the main queue.
 */
final class HttpApi {
    private let session = URLSession(configuration: .default)
    private var tasks: [Int64: URLSessionDataTask] = [:]
    private let emit: ([String: Any]) -> Void

    init(emit: @escaping ([String: Any]) -> Void) {
        self.emit = emit
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "request":
            request(args)
            return true
        case "cancel":
            if let id = (args["id"] as? NSNumber)?.int64Value {
                tasks[id]?.cancel()
            }
            return true
        default:
            return false
        }
    }

    func dispose() {
        session.invalidateAndCancel()
        tasks.removeAll()
    }

    private func request(_ args: [String: Any]) {
        let id = (args["id"] as? NSNumber)?.int64Value ?? 0
        guard
            let raw = args["url"] as? String,
            let url = URL(string: raw),
            url.scheme != nil
        else {
            emit(["id": id, "ok": false, "error": "invalid_url"])
            return
        }

        var request = URLRequest(url: url)
        request.httpMethod = (args["method"] as? String ?? "GET").uppercased()
        let timeoutMs = (args["timeoutMs"] as? NSNumber)?.doubleValue ?? 30_000
        request.timeoutInterval = timeoutMs / 1000
        if let headers = args["headers"] as? [String: Any] {
            for (name, value) in headers {
                if let value = value as? String {
                    request.setValue(value, forHTTPHeaderField: name)
                }
            }
        }
        if let body = args["body"] as? String {
            request.httpBody = Data(body.utf8)
        }

        let task = session.dataTask(with: request) { [weak self] data, response, error in
            var payload: [String: Any] = ["id": id]
            if let error {
                payload["ok"] = false
                payload["error"] = HttpApi.code(for: error)
            } else if let http = response as? HTTPURLResponse {
                var headers: [String: String] = [:]
                for (name, value) in http.allHeaderFields {
                    if let name = name as? String {
                        headers[name.lowercased()] = "\(value)"
                    }
                }
                payload["ok"] = true
                payload["status"] = http.statusCode
                payload["headers"] = headers
                payload["body"] = String(decoding: data ?? Data(), as: UTF8.self)
            } else {
                payload["ok"] = false
                payload["error"] = "network"
            }

            let result = payload
            DispatchQueue.main.async {
                guard let self else { return }
                self.tasks[id] = nil
                self.emit(result)
            }
        }
        tasks[id] = task
        task.resume()
    }

    private static func code(for error: Error) -> String {
        guard let urlError = error as? URLError else {
            return "network"
        }
        switch urlError.code {
        case .timedOut:
            return "timeout"
        case .cancelled:
            return "cancelled"
        case .badURL, .unsupportedURL:
            return "invalid_url"
        default:
            return "network"
        }
    }
}
